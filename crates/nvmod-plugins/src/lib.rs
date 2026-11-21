// SPDX-License-Identifier: GPL-3.0-only
//! The plugin list of an nvmod profile, read and edited by hand: `nvmod plugins list|enable|disable|move|lock|unlock`.
//!
//! Every write goes through `nvmod_loadorder::write_order`: `plugins.txt` (Windows-1252), `loadorder.txt` (and
//! `lockedorder.txt` for a lock) in one journaled step, then the file times of the staged plugins, so `nvmod
//! rollback` restores the files byte for byte. The forced primary plugins (`FalloutNV.esm`, and the TTW list for
//! a TTW profile) stay first and cannot be moved, disabled or locked.

use nvmod_core::Provider;
use nvmod_formats::LockedOrder;
use nvmod_instance::NvmodInstance;
use nvmod_loadorder::{Error, Slot, primary_plugins, read_locks, read_slots};
use schemars::JsonSchema;
use serde::Serialize;

/// The version of the JSON layout of [`ListReport`].
pub const SCHEMA_VERSION: u32 = 1;

/// Why a command did not run (exit code 2) or failed after it started (exit code 4).
#[derive(Debug, PartialEq, Eq)]
pub enum PluginsError {
    /// Refused before anything was written.
    Refused(String),
    /// A step failed and was rolled back.
    Failed(String),
}

impl From<Error> for PluginsError {
    fn from(e: Error) -> Self {
        match e {
            Error::Refused(why) => PluginsError::Refused(why),
            Error::Failed(why) => PluginsError::Failed(why),
        }
    }
}

fn refuse<T>(why: impl Into<String>) -> Result<T, PluginsError> {
    Err(PluginsError::Refused(why.into()))
}

/// One master a plugin declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct MasterRow {
    /// The master's file name as the header spells it.
    pub name: String,
    /// A Mod or the base copy provides the file.
    pub present: bool,
    /// The master is listed active in the profile.
    pub active: bool,
}

/// One plugin of the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PluginRow {
    /// The file name as the lists spell it.
    pub name: String,
    /// Listed in `plugins.txt`.
    pub active: bool,
    /// 1 is the first plugin to load.
    pub position: usize,
    /// The 1-based position `lockedorder.txt` holds it at.
    pub locked: Option<usize>,
    /// A forced primary plugin: always first, not movable.
    pub forced: bool,
    /// A Mod or the base copy provides the file.
    pub installed: bool,
    /// The Mod (or `Overwrite`, or `the base copy`) whose file wins.
    pub provider: Option<String>,
    /// The masters its header declares; empty when it is not installed or its header cannot be read.
    pub masters: Vec<MasterRow>,
    /// How many masters are not present.
    pub masters_missing: usize,
    /// Why the header could not be read, if it could not.
    pub header_error: Option<String>,
}

/// One entry of `lockedorder.txt`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct LockRow {
    /// The plugin file name as the file spells it.
    pub name: String,
    /// The 1-based position it is held at.
    pub position: usize,
    /// The plugin is in the list (an entry for an absent plugin is kept).
    pub listed: bool,
}

/// `nvmod plugins list --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ListReport {
    /// The version of this layout.
    pub schema_version: u32,
    /// The profile.
    pub profile: String,
    /// The plugins in load order.
    pub plugins: Vec<PluginRow>,
    /// Every lock, including those of plugins that are not in the list.
    pub locks: Vec<LockRow>,
}

/// The JSON Schema of [`ListReport`], as pretty JSON with a trailing newline (`schemas/plugins-list.v1.json`).
///
/// # Panics
/// Never in practice: a generated schema always serialises.
#[must_use]
pub fn list_schema() -> String {
    let mut text = serde_json::to_string_pretty(&schemars::schema_for!(ListReport))
        .expect("a schema serialises");
    text.push('\n');
    text
}

/// What the plugin files and the installed Mods say, read once.
struct Context<'a> {
    instance: &'a NvmodInstance,
    profile: String,
    slots: Vec<Slot>,
    locks: LockedOrder,
    index: Option<nvmod_core::FileIndex>,
}

impl<'a> Context<'a> {
    fn open(instance: &'a NvmodInstance, profile: &str) -> Result<Self, PluginsError> {
        let dir = instance.profile_dir(profile);
        if !dir.is_dir() {
            return refuse(format!("the Instance has no profile `{profile}`"));
        }
        let slots = read_slots(&dir)?;
        let locks = read_locks(&dir)?;
        let index = nvmod_conflicts::scan(instance, profile, true)
            .ok()
            .map(|layers| nvmod_conflicts::index_of(&layers));
        Ok(Self {
            instance,
            profile: profile.to_owned(),
            slots,
            locks,
            index,
        })
    }

    fn provider(&self, name: &str) -> Option<&Provider> {
        self.index.as_ref()?.winner(name)
    }

    fn installed(&self, name: &str) -> bool {
        self.provider(name).is_some()
    }

    /// The masters of `name`'s winning file: `None` when it is not installed, `Err` when the header is unreadable.
    fn masters(&self, name: &str) -> Option<Result<Vec<String>, String>> {
        let path = nvmod_conflicts::provider_file(self.instance, self.provider(name)?)?;
        Some(nvmod_loadorder::masters_of(&path))
    }
}

fn find(order: &[Slot], name: &str) -> Option<usize> {
    order.iter().position(|s| s.name.eq_ignore_ascii_case(name))
}

fn is_active(order: &[Slot], name: &str) -> bool {
    find(order, name).is_some_and(|i| order[i].active)
}

/// The plugin list of `profile` with masters, providers and locks.
///
/// # Errors
/// `Refused` when the profile or a list cannot be read.
pub fn list(instance: &NvmodInstance, profile: &str) -> Result<ListReport, PluginsError> {
    let ctx = Context::open(instance, profile)?;
    let forced = primary_plugins(&ctx.slots);
    let plugins = ctx
        .slots
        .iter()
        .enumerate()
        .map(|(i, slot)| {
            let (masters, header_error) = match ctx.masters(&slot.name) {
                Some(Ok(names)) => (names, None),
                Some(Err(why)) => (Vec::new(), Some(why)),
                None => (Vec::new(), None),
            };
            let masters: Vec<MasterRow> = masters
                .into_iter()
                .map(|name| MasterRow {
                    present: ctx.installed(&name),
                    active: is_active(&ctx.slots, &name),
                    name,
                })
                .collect();
            PluginRow {
                name: slot.name.clone(),
                active: slot.active,
                position: i + 1,
                locked: ctx.locks.get(&slot.name).map(|p| p as usize + 1),
                forced: forced.iter().any(|f| f.eq_ignore_ascii_case(&slot.name)),
                installed: ctx.installed(&slot.name),
                provider: ctx.provider(&slot.name).map(|p| p.source.clone()),
                masters_missing: masters.iter().filter(|m| !m.present).count(),
                masters,
                header_error,
            }
        })
        .collect();
    let locks = ctx
        .locks
        .entries
        .iter()
        .map(|l| LockRow {
            name: l.name.clone(),
            position: l.priority as usize + 1,
            listed: find(&ctx.slots, &l.name).is_some(),
        })
        .collect();
    Ok(ListReport {
        schema_version: SCHEMA_VERSION,
        profile: profile.to_owned(),
        plugins,
        locks,
    })
}

/// Where `move` puts a plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// First, after the forced primary plugins.
    Top,
    /// Last.
    Bottom,
    /// Right before this plugin.
    Before(String),
    /// Right after this plugin.
    After(String),
}

/// One edit of the plugin list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// Activate plugins; with `with_masters` also the masters they need, in order.
    Enable {
        /// The plugins.
        names: Vec<String>,
        /// Enable missing masters too.
        with_masters: bool,
    },
    /// Deactivate plugins.
    Disable(Vec<String>),
    /// Move one plugin.
    Move {
        /// The plugin.
        name: String,
        /// Where to.
        to: Place,
    },
    /// Hold a plugin at a 1-based position (it is moved there now and kept there by `sort`).
    Lock {
        /// The plugin.
        name: String,
        /// The 1-based position.
        position: usize,
    },
    /// Remove a lock.
    Unlock(String),
}

impl Edit {
    /// The word of the sub-command.
    #[must_use]
    pub fn action(&self) -> &'static str {
        match self {
            Edit::Enable { .. } => "enable",
            Edit::Disable(_) => "disable",
            Edit::Move { .. } => "move",
            Edit::Lock { .. } => "lock",
            Edit::Unlock(_) => "unlock",
        }
    }
}

/// One plugin of the list after an edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrderRow {
    /// 1 is the first.
    pub position: usize,
    /// The file name.
    pub name: String,
    /// Active.
    pub active: bool,
}

/// What an edit did (or would do).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EditReport {
    /// `changed`, `unchanged` or `dry-run`.
    pub status: &'static str,
    /// True for a dry run.
    pub dry_run: bool,
    /// `enable`, `disable`, `move`, `lock` or `unlock`.
    pub action: &'static str,
    /// The profile.
    pub profile: String,
    /// The plugins whose state, place or lock changed, in the order they changed.
    pub changed: Vec<String>,
    /// The list after the edit.
    pub order: Vec<OrderRow>,
    /// `plugins.txt`, `loadorder.txt` or `lockedorder.txt` was written.
    pub lists_written: bool,
    /// Plugin files that got a new modification time.
    pub stamped: usize,
    /// The journal `nvmod rollback` undoes; absent for a dry run or when nothing changed.
    pub journal: Option<String>,
}

/// The names as the list spells them, or a refusal for one that is not in it.
fn listed<'a>(order: &'a [Slot], name: &str) -> Result<&'a Slot, PluginsError> {
    match find(order, name) {
        Some(i) => Ok(&order[i]),
        None => refuse(format!("`{name}` is not in the Load order of this profile")),
    }
}

fn forced_refusal(order: &[Slot], name: &str, what: &str) -> Result<(), PluginsError> {
    if primary_plugins(order)
        .iter()
        .any(|f| f.eq_ignore_ascii_case(name))
    {
        return refuse(format!(
            "`{name}` is a forced primary plugin: it always loads first and cannot be {what}"
        ));
    }
    Ok(())
}

/// Where an unlisted master of the base game goes: right after the last listed master that the game loads before
/// it (the front when none is listed), so a Profile that lacks the DLC gets them back in the game's order.
/// `None` for any other plugin.
fn base_master_place(order: &[Slot], name: &str) -> Option<usize> {
    let rank = nvmod_instance::BASE_MASTERS
        .iter()
        .position(|m| m.eq_ignore_ascii_case(name))?;
    let before = nvmod_instance::BASE_MASTERS[..rank]
        .iter()
        .filter_map(|m| find(order, m))
        .max();
    Some(before.map_or(0, |i| i + 1))
}

fn enable_one(
    ctx: &Context,
    order: &mut Vec<Slot>,
    name: &str,
    with_masters: bool,
    changed: &mut Vec<String>,
    visiting: &mut Vec<String>,
) -> Result<(), PluginsError> {
    if is_active(order, name) {
        return Ok(());
    }
    if visiting.iter().any(|v| v.eq_ignore_ascii_case(name)) {
        return refuse(format!(
            "`{name}` is a master of itself through its masters"
        ));
    }
    let listed_at = find(order, name);
    let canonical = match listed_at {
        Some(i) => order[i].name.clone(),
        None => match ctx.provider(name) {
            Some(p) => p.path.rsplit('/').next().unwrap_or(name).to_owned(),
            None => {
                return refuse(format!(
                    "`{name}` is not in the Load order and no Mod provides it"
                ));
            }
        },
    };
    if !ctx.installed(&canonical) {
        return refuse(format!(
            "`{canonical}` is not installed (no Mod or base file provides it), so it cannot be enabled"
        ));
    }
    let masters = match ctx.masters(&canonical) {
        Some(Ok(masters)) => masters,
        // An unreadable header cannot be checked; the Doctor reports it.
        _ => Vec::new(),
    };
    let unmet: Vec<&String> = masters
        .iter()
        .filter(|m| !m.eq_ignore_ascii_case(&canonical) && !is_active(order, m))
        .collect();
    if !unmet.is_empty() && !with_masters {
        let words: Vec<String> = unmet
            .iter()
            .map(|m| {
                if find(order, m).is_some() {
                    format!("{m} (in the list but not active)")
                } else {
                    format!("{m} (not in the list)")
                }
            })
            .collect();
        return refuse(format!(
            "`{canonical}` needs master(s) that are not active: {}; enable them first or pass --with-masters",
            words.join(", ")
        ));
    }
    visiting.push(canonical.clone());
    for master in &unmet {
        if !ctx.installed(master) {
            return refuse(format!(
                "master {master} of `{canonical}` is not installed, so it cannot be enabled"
            ));
        }
        enable_one(ctx, order, master, true, changed, visiting)?;
    }
    visiting.pop();
    // The plugin goes at the end when the list does not hold it yet (its masters are all before it).
    let at = if let Some(i) = find(order, &canonical) {
        i
    } else {
        let at = base_master_place(order, &canonical).unwrap_or(order.len());
        order.insert(
            at,
            Slot {
                name: canonical.clone(),
                active: false,
            },
        );
        at
    };
    order[at].active = true;
    // Masters that load after the plugin move to just before it.
    for master in &masters {
        let (Some(m), Some(p)) = (find(order, master), find(order, &canonical)) else {
            continue;
        };
        if m > p && unmet.iter().any(|u| u.eq_ignore_ascii_case(master)) {
            let slot = order.remove(m);
            order.insert(p, slot);
        }
    }
    changed.push(canonical);
    Ok(())
}

fn dependants(ctx: &Context, order: &[Slot], name: &str, ignoring: &[String]) -> Vec<String> {
    order
        .iter()
        .filter(|s| {
            s.active
                && !s.name.eq_ignore_ascii_case(name)
                && !ignoring.iter().any(|i| i.eq_ignore_ascii_case(&s.name))
        })
        .filter(|s| {
            matches!(ctx.masters(&s.name), Some(Ok(m)) if m.iter().any(|m| m.eq_ignore_ascii_case(name)))
        })
        .map(|s| s.name.clone())
        .collect()
}

/// Names of plugins that the order of `name` breaks: a master of it that loads after it, or a plugin that has it
/// as a master and loads before it.
fn order_problems(ctx: &Context, order: &[Slot], name: &str) -> Vec<String> {
    let Some(at) = find(order, name) else {
        return Vec::new();
    };
    let mut problems = Vec::new();
    if let Some(Ok(masters)) = ctx.masters(name) {
        for master in masters {
            if find(order, &master).is_some_and(|m| m > at) {
                problems.push(format!("its master {master} would load after it"));
            }
        }
    }
    for slot in order.iter().take(at) {
        if matches!(ctx.masters(&slot.name), Some(Ok(m)) if m.iter().any(|m| m.eq_ignore_ascii_case(name)))
        {
            problems.push(format!(
                "{} (which needs it as a master) would load before it",
                slot.name
            ));
        }
    }
    problems
}

fn move_to(order: &mut Vec<Slot>, name: &str, to: &Place) -> Result<(), PluginsError> {
    let at = find(order, name).ok_or_else(|| {
        PluginsError::Refused(format!("`{name}` is not in the Load order of this profile"))
    })?;
    forced_refusal(order, name, "moved")?;
    let forced = primary_plugins(order).len();
    let slot = order.remove(at);
    let target = match to {
        Place::Top => forced,
        Place::Bottom => order.len(),
        Place::Before(other) | Place::After(other) => {
            let Some(i) = find(order, other) else {
                order.insert(at, slot);
                return refuse(if other.eq_ignore_ascii_case(name) {
                    format!("`{name}` cannot be moved relative to itself")
                } else {
                    format!("`{other}` is not in the Load order of this profile")
                });
            };
            if matches!(to, Place::Before(_)) {
                if i < forced {
                    order.insert(at, slot);
                    return refuse(format!(
                        "`{other}` is a forced primary plugin: nothing can load before it"
                    ));
                }
                i
            } else {
                i + 1
            }
        }
    };
    order.insert(target.max(forced).min(order.len()), slot);
    Ok(())
}

/// The list, the locks and what changed, after one edit.
struct Edited {
    order: Vec<Slot>,
    locks: LockedOrder,
    changed: Vec<String>,
}

fn disable(ctx: &Context, names: &[String], e: &mut Edited) -> Result<(), PluginsError> {
    let canon: Vec<String> = names
        .iter()
        .map(|n| listed(&e.order, n).map(|s| s.name.clone()))
        .collect::<Result<_, _>>()?;
    for name in &canon {
        forced_refusal(&e.order, name, "disabled")?;
    }
    for name in canon.iter().filter(|n| is_active(&e.order, n)) {
        let dep = dependants(ctx, &e.order, name, &canon);
        if !dep.is_empty() {
            return refuse(format!(
                "`{name}` is a master of active plugin(s) {}; disable those first",
                dep.join(", ")
            ));
        }
    }
    for name in canon {
        let i = find(&e.order, &name).unwrap_or_default();
        if e.order[i].active {
            e.order[i].active = false;
            e.changed.push(name);
        }
    }
    Ok(())
}

fn move_plugin(ctx: &Context, name: &str, to: &Place, e: &mut Edited) -> Result<(), PluginsError> {
    let canonical = listed(&e.order, name)?.name.clone();
    if e.locks.get(&canonical).is_some() {
        return refuse(format!(
            "`{canonical}` is locked; unlock it before moving it"
        ));
    }
    let before = e.order.clone();
    move_to(&mut e.order, &canonical, to)?;
    let problems = order_problems(ctx, &e.order, &canonical);
    if !problems.is_empty() {
        return refuse(format!(
            "moving `{canonical}` there breaks the master order: {}",
            problems.join("; ")
        ));
    }
    if e.order != before {
        e.changed.push(canonical);
    }
    Ok(())
}

fn lock_plugin(
    ctx: &Context,
    name: &str,
    position: usize,
    e: &mut Edited,
) -> Result<(), PluginsError> {
    let canonical = listed(&e.order, name)?.name.clone();
    forced_refusal(&e.order, &canonical, "locked")?;
    let forced = primary_plugins(&e.order).len();
    if position <= forced || position > e.order.len() {
        return refuse(format!(
            "position {position} is outside {}..={} (the first {forced} place(s) belong to the forced primary plugins)",
            forced + 1,
            e.order.len()
        ));
    }
    let from = find(&e.order, &canonical).unwrap_or_default();
    let slot = e.order.remove(from);
    e.order.insert(position - 1, slot);
    let problems = order_problems(ctx, &e.order, &canonical);
    if !problems.is_empty() {
        return refuse(format!(
            "locking `{canonical}` at {position} breaks the master order: {}",
            problems.join("; ")
        ));
    }
    let priority = u32::try_from(position - 1).unwrap_or(u32::MAX);
    if e.locks.get(&canonical) != Some(priority) || from != position - 1 {
        e.changed.push(canonical.clone());
    }
    e.locks.set(&canonical, priority);
    Ok(())
}

fn edited(ctx: &Context, edit: &Edit) -> Result<Edited, PluginsError> {
    let mut e = Edited {
        order: ctx.slots.clone(),
        locks: ctx.locks.clone(),
        changed: Vec::new(),
    };
    match edit {
        Edit::Enable {
            names,
            with_masters,
        } => {
            for name in names {
                let mut changed = Vec::new();
                enable_one(
                    ctx,
                    &mut e.order,
                    name,
                    *with_masters,
                    &mut changed,
                    &mut Vec::new(),
                )?;
                e.changed.extend(changed);
            }
        }
        Edit::Disable(names) => disable(ctx, names, &mut e)?,
        Edit::Move { name, to } => move_plugin(ctx, name, to, &mut e)?,
        Edit::Lock { name, position } => lock_plugin(ctx, name, *position, &mut e)?,
        Edit::Unlock(name) => {
            let known = e
                .locks
                .entries
                .iter()
                .find(|l| l.name.eq_ignore_ascii_case(name))
                .map(|l| l.name.clone());
            if let Some(known) = known {
                e.locks.remove(&known);
                e.changed.push(known);
            }
        }
    }
    Ok(e)
}

/// Apply `edit` to the plugin list of `profile`, or only plan it with `dry_run`.
///
/// # Errors
/// `Refused` (nothing was written) for an unknown plugin, a missing master, a dependant, a forced or locked
/// plugin, a master order that would break, or a name `plugins.txt` cannot hold; `Failed` when a write step failed
/// and was rolled back.
pub fn apply(
    instance: &NvmodInstance,
    profile: &str,
    edit: &Edit,
    dry_run: bool,
) -> Result<EditReport, PluginsError> {
    let ctx = Context::open(instance, profile)?;
    let Edited {
        order,
        locks,
        changed,
    } = edited(&ctx, edit)?;
    let lock_changed = locks != ctx.locks;
    let order_changed = order != ctx.slots;
    let report = |status, written: Option<nvmod_loadorder::Applied>| EditReport {
        status,
        dry_run,
        action: edit.action(),
        profile: profile.to_owned(),
        changed: changed.clone(),
        order: order
            .iter()
            .enumerate()
            .map(|(i, s)| OrderRow {
                position: i + 1,
                name: s.name.clone(),
                active: s.active,
            })
            .collect(),
        lists_written: written.as_ref().is_some_and(|w| w.lists_written),
        stamped: written.as_ref().map_or(0, |w| w.stamped),
        journal: written.and_then(|w| w.journal),
    };
    if !lock_changed && !order_changed {
        return Ok(report("unchanged", None));
    }
    // Refuse a name the Windows code page lacks before anything is written (also for a dry run).
    nvmod_loadorder::list_bytes(&order)?;
    if dry_run {
        return Ok(report("dry-run", None));
    }
    let staging = instance.staging_dir(profile).join("data");
    let locked_bytes = lock_changed.then(|| locks.render());
    let written = nvmod_loadorder::write_order(
        instance,
        &ctx.profile,
        staging.is_dir().then_some(staging.as_path()),
        &order,
        locked_bytes.as_deref(),
        nvmod_journal::KIND_PLUGINS,
    )?;
    Ok(report("changed", Some(written)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checked_in_list_schema_is_the_generated_one() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas/plugins-list.v1.json");
        if std::env::var_os("NVMOD_BLESS").is_some() {
            std::fs::write(&path, list_schema()).unwrap();
        }
        let checked_in = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            checked_in == list_schema(),
            "schemas/plugins-list.v1.json differs from the generated schema; bump the schema version if the \
             change is intended and run the tests with NVMOD_BLESS=1 to rewrite it"
        );
    }

    #[test]
    fn move_places_relative_to_the_forced_block() {
        let slot = |n: &str| Slot {
            name: n.to_owned(),
            active: true,
        };
        let mut order = vec![
            slot("FalloutNV.esm"),
            slot("A.esp"),
            slot("B.esp"),
            slot("C.esp"),
        ];
        move_to(&mut order, "C.esp", &Place::Top).unwrap();
        let names = |o: &[Slot]| o.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&order), ["FalloutNV.esm", "C.esp", "A.esp", "B.esp"]);
        move_to(&mut order, "C.esp", &Place::After("B.esp".into())).unwrap();
        assert_eq!(names(&order), ["FalloutNV.esm", "A.esp", "B.esp", "C.esp"]);
        move_to(&mut order, "A.esp", &Place::Bottom).unwrap();
        assert_eq!(names(&order), ["FalloutNV.esm", "B.esp", "C.esp", "A.esp"]);
        assert!(move_to(&mut order, "A.esp", &Place::Before("FalloutNV.esm".into())).is_err());
        assert!(move_to(&mut order, "FalloutNV.esm", &Place::Bottom).is_err());
        assert!(move_to(&mut order, "A.esp", &Place::Before("Nope.esp".into())).is_err());
        assert_eq!(names(&order), ["FalloutNV.esm", "B.esp", "C.esp", "A.esp"]);
    }
}
