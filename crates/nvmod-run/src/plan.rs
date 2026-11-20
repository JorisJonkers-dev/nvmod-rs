// SPDX-License-Identifier: GPL-3.0-only
//! The launch plan of one `nvmod run`: what to start, with which arguments, folder and environment, and what is
//! done to the Staging tree first. Nothing here starts a program or writes a file.
//!
//! The game entry comes from `nvmod_instance::launch_plan`, the same function `nvmod verify` prints, so the two
//! agree by construction.

use crate::launch::LaunchSpec;
use crate::native::{self, Native};
use nvmod_core::Executable;
use nvmod_instance::{NvmodInstance, launch_plan};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The game's Steam app id (Fallout: New Vegas), used when an entry names none.
pub const GAME_APP_ID: u32 = 22380;
/// Built-in entry: the game through xNVSE or vanilla, as `nvmod verify` plans it.
pub const ENTRY_GAME: &str = "game";
/// Built-in entry: the installed nv-rs program with the staged Data folder.
pub const ENTRY_NV_RS: &str = "nv-rs";

/// The operating system family a plan is made for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Windows programs run through Proton.
    Linux,
    /// Programs run directly.
    Windows,
}

impl Platform {
    /// The platform this build runs on.
    #[must_use]
    pub fn host() -> Platform {
        if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }
}

/// What `nvmod run` does to the Staging tree before starting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum StagingAction {
    /// There is no tree yet: build it.
    Build,
    /// There is a tree that is older than its inputs, or `--restage` was given: build it again.
    Rebuild {
        /// Why.
        reason: String,
    },
    /// The tree is newer than every input: leave it.
    Current,
    /// `--no-stage`: leave it as it is, whatever state it is in.
    Skipped,
}

impl StagingAction {
    /// Whether the tree is built or rebuilt.
    #[must_use]
    pub fn builds(&self) -> bool {
        matches!(self, StagingAction::Build | StagingAction::Rebuild { .. })
    }

    /// One line for people.
    #[must_use]
    pub fn describe(&self, profile: &str) -> String {
        match self {
            StagingAction::Build => {
                format!("build the Staging tree of profile `{profile}` (none yet)")
            }
            StagingAction::Rebuild { reason } => {
                format!("rebuild the Staging tree of profile `{profile}` ({reason})")
            }
            StagingAction::Current => {
                format!("keep the Staging tree of profile `{profile}` (newer than its inputs)")
            }
            StagingAction::Skipped => {
                format!("leave the Staging tree of profile `{profile}` as it is (--no-stage)")
            }
        }
    }
}

/// The plan of one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunPlan {
    /// The entry that was chosen: `game`, `nv-rs`, a registry title or the path given.
    pub entry: String,
    /// `game`, `nv-rs`, `registry` or `path`.
    pub kind: &'static str,
    /// What to start.
    pub command: LaunchSpec,
    /// Whether the program is a Windows program started through Proton.
    pub via_proton: bool,
    /// The staged `Data` folder the program is told about (`NVMOD_DATA_DIR`).
    pub data_dir: PathBuf,
    /// What happens to the Staging tree first.
    pub staging: StagingAction,
    /// The run folder, user data and log of a native nv-rs run (`None` for every other entry).
    pub native: Option<Native>,
    /// Things worth knowing about this run.
    pub notes: Vec<String>,
}

/// What to plan.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    /// The Instance.
    pub instance: &'a NvmodInstance,
    /// The profile whose Staging tree the program sees.
    pub profile: &'a str,
    /// `game`, `nv-rs`, a registry title or a path to a program.
    pub target: &'a str,
    /// Arguments added after the entry's own.
    pub args: &'a [String],
    /// The Proton program (`proton` on `PATH` when `None`).
    pub proton: Option<&'a str>,
    /// The platform to plan for.
    pub platform: Platform,
    /// The Steam client folder (`STEAM_COMPAT_CLIENT_INSTALL_PATH`), when known.
    pub steam_client: Option<PathBuf>,
    /// `game` on an nv-rs Instance runs through Proton instead of natively.
    pub via_proton: bool,
    /// The nv-viewer to start, instead of the one found.
    pub viewer: Option<PathBuf>,
    /// The folder of shared nv-rs builds (`~/.local/opt/nv-rs`), searched when the Instance has none of its own.
    pub shared_root: Option<PathBuf>,
}

/// `path` as Wine names it: `Z:` and backslashes.
#[must_use]
pub fn wine_path(path: &Path) -> String {
    format!("Z:{}", path.to_string_lossy().replace('/', "\\"))
}

/// Split `text` on spaces, a double quote grouping words (`-a "b c"` is `-a` and `b c`). A backslash is an
/// ordinary character, so Windows paths survive.
#[must_use]
pub fn split_arguments(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// The staged `Data` folder of `profile`.
#[must_use]
pub fn data_dir(instance: &NvmodInstance, profile: &str) -> PathBuf {
    instance.staging_dir(profile).join("data")
}

fn absolute(instance: &NvmodInstance, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        instance.root.join(p)
    }
}

fn base_env(req: &Request, data: &Path) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "NVMOD_DATA_DIR".to_owned(),
            data.to_string_lossy().into_owned(),
        ),
        (
            "NVMOD_INSTANCE".to_owned(),
            req.instance.root.to_string_lossy().into_owned(),
        ),
        ("NVMOD_PROFILE".to_owned(), req.profile.to_owned()),
    ])
}

fn proton_env(req: &Request, env: &mut BTreeMap<String, String>, app_id: u32) {
    env.insert(
        "STEAM_COMPAT_DATA_PATH".to_owned(),
        req.instance
            .root
            .join("compatdata")
            .to_string_lossy()
            .into_owned(),
    );
    env.insert("SteamAppId".to_owned(), app_id.to_string());
    env.insert("SteamGameId".to_owned(), app_id.to_string());
    if let Some(steam) = &req.steam_client {
        env.insert(
            "STEAM_COMPAT_CLIENT_INSTALL_PATH".to_owned(),
            steam.to_string_lossy().into_owned(),
        );
    }
}

/// Plan one run (the staging action is [`StagingAction::Current`] here; the caller sets the real one).
///
/// # Errors
/// A refusal in words: no such entry, a program that does not exist, nv-rs not installed, a Proton path that
/// does not exist, or an instance without a launchable game.
pub fn plan(req: &Request) -> Result<RunPlan, String> {
    if let Some(p) = req.proton
        && p.contains(['/', '\\'])
        && !Path::new(p).is_file()
    {
        return Err(format!("--proton {p} is not a file"));
    }
    if req.target.eq_ignore_ascii_case(ENTRY_GAME) {
        // An nv-rs Instance on Linux runs the game natively; `--via-proton` is the fallback.
        let native = req.platform == Platform::Linux
            && req.instance.config.target == nvmod_core::Target::NvRs
            && !req.via_proton;
        return if native {
            plan_nv_rs(req)
        } else {
            plan_game(req)
        };
    }
    if req.target.eq_ignore_ascii_case(ENTRY_NV_RS) {
        return plan_nv_rs(req);
    }
    let registry = req
        .instance
        .config
        .executables
        .iter()
        .find(|e| e.title.eq_ignore_ascii_case(req.target));
    match registry {
        Some(e) => plan_program(req, "registry", e),
        None if req.target.contains(['/', '\\']) || Path::new(req.target).is_file() => {
            plan_program(
                req,
                "path",
                &Executable {
                    title: req.target.to_owned(),
                    binary: req.target.to_owned(),
                    arguments: String::new(),
                    working_directory: None,
                    steam_app_id: None,
                    hidden: false,
                },
            )
        }
        None => {
            let titles = req
                .instance
                .config
                .executables
                .iter()
                .fold(String::new(), |s, e| s + ", " + &e.title);
            Err(format!(
                "no entry titled `{}`; the entries are: {ENTRY_GAME}, {ENTRY_NV_RS}{titles}",
                req.target
            ))
        }
    }
}

fn plan_game(req: &Request) -> Result<RunPlan, String> {
    let data = data_dir(req.instance, req.profile);
    let mut env = base_env(req, &data);
    let launch = launch_plan(req.instance, req.proton);
    let want = if req.platform == Platform::Linux {
        "linux"
    } else {
        "windows"
    };
    let entry = launch
        .entries
        .iter()
        .find(|e| e.platform == want)
        .ok_or("the launch plan has no entry for this platform")?;
    let via_proton = req.platform == Platform::Linux;
    env.extend(entry.env.clone());
    let mut notes = vec![launch.note.clone()];
    if via_proton {
        proton_env(req, &mut env, GAME_APP_ID);
        notes.push(format!(
            "the game reads the base copy's Data folder; the staged Data folder is {} (NVMOD_DATA_DIR), and binding it as the game's Data waits for the overlay backend behind the Deploy design",
            data.display()
        ));
    }
    let mut args = entry.args.clone();
    args.extend(req.args.iter().cloned());
    Ok(RunPlan {
        entry: ENTRY_GAME.to_owned(),
        kind: "game",
        command: LaunchSpec {
            program: entry.program.clone(),
            args,
            env,
            working_dir: PathBuf::from(&entry.working_dir),
            log: None,
        },
        via_proton,
        data_dir: data,
        staging: StagingAction::Current,
        native: None,
        notes,
    })
}

fn plan_nv_rs(req: &Request) -> Result<RunPlan, String> {
    let data = data_dir(req.instance, req.profile);
    let windows = req.platform == Platform::Windows;
    let suffix = if windows { ".exe" } else { "" };
    let viewer = native::find_viewer(
        req.instance,
        req.viewer.as_deref(),
        req.shared_root.as_deref(),
        suffix,
    )?;
    let mut env = base_env(req, &data);
    if windows {
        // No run folder off Linux: the viewer reads the staged Data folder and the executable beside it.
        let mut args = vec![data.to_string_lossy().into_owned()];
        args.extend(req.args.iter().cloned());
        return Ok(RunPlan {
            entry: ENTRY_NV_RS.to_owned(),
            kind: "nv-rs",
            command: LaunchSpec {
                program: viewer.to_string_lossy().into_owned(),
                args,
                env,
                working_dir: req.instance.nv_rs_dir(),
                log: None,
            },
            via_proton: false,
            data_dir: data,
            staging: StagingAction::Current,
            native: None,
            notes: vec![
                "nv-rs starts with the staged Data folder; no executable is copied".to_owned(),
            ],
        });
    }
    let run = native::run_dir(req.instance, req.profile);
    let userdata = run.join("userdata");
    let log = native::log_path();
    // The game window and the log are what a person playing needs; a caller's own values win.
    let defaults = [
        ("NV_D3D9", "wgpu"),
        // The engine plays sound only when asked: without this `DirectSoundCreate8` answers "no driver".
        ("NV_SOUND", "out"),
        ("RUST_BACKTRACE", "1"),
        ("RUST_LOG", "info"),
    ];
    for (k, v) in defaults {
        if std::env::var_os(k).is_none() {
            env.insert(k.to_owned(), v.to_owned());
        }
    }
    env.insert(
        "NV_LINUX_PROFILE".to_owned(),
        run.join("linux_profile.txt").to_string_lossy().into_owned(),
    );
    let mut args = vec![run.to_string_lossy().into_owned()];
    args.extend(req.args.iter().cloned());
    Ok(RunPlan {
        entry: ENTRY_NV_RS.to_owned(),
        kind: "nv-rs",
        command: LaunchSpec {
            program: viewer.to_string_lossy().into_owned(),
            args,
            env,
            working_dir: run.clone(),
            log: Some(log.clone()),
        },
        via_proton: false,
        data_dir: data,
        staging: StagingAction::Current,
        native: Some(Native {
            run_dir: run,
            userdata,
            log,
            viewer,
        }),
        notes: vec![
            "nv-rs starts natively from the run folder: its Data is the staged tree, its FalloutNV.exe a link to the base copy's; no game file is copied and the Steam folder is not touched".to_owned(),
            "the profile's INIs, plugins.txt and loadorder.txt are placed in the run folder's user data; plugin order comes from file times (`nvmod sort --apply` sets them)".to_owned(),
        ],
    })
}

fn plan_program(req: &Request, kind: &'static str, entry: &Executable) -> Result<RunPlan, String> {
    let data = data_dir(req.instance, req.profile);
    let mut env = base_env(req, &data);
    let binary = if kind == "path" {
        std::path::absolute(&entry.binary).unwrap_or_else(|_| PathBuf::from(&entry.binary))
    } else {
        absolute(req.instance, &entry.binary)
    };
    if !binary.is_file() {
        return Err(format!(
            "the program of `{}` ({}) does not exist",
            entry.title,
            binary.display()
        ));
    }
    let working_dir = entry.working_directory.as_deref().map_or_else(
        || {
            binary
                .parent()
                .map_or_else(|| req.instance.root.clone(), Path::to_path_buf)
        },
        |w| absolute(req.instance, w),
    );
    let is_exe = binary
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("exe"));
    let via_proton = req.platform == Platform::Linux && is_exe;
    let expand = |word: &str| {
        let (data_text, instance_text) = if via_proton {
            (wine_path(&data), wine_path(&req.instance.root))
        } else {
            (
                data.to_string_lossy().into_owned(),
                req.instance.root.to_string_lossy().into_owned(),
            )
        };
        word.replace("%DATA%", &data_text)
            .replace("%INSTANCE%", &instance_text)
            .replace("%PROFILE%", req.profile)
    };
    let mut tool_args: Vec<String> = split_arguments(&entry.arguments)
        .iter()
        .map(|w| expand(w))
        .collect();
    tool_args.extend(req.args.iter().cloned());
    let mut notes = Vec::new();
    let (program, args) = if via_proton {
        proton_env(req, &mut env, entry.steam_app_id.unwrap_or(GAME_APP_ID));
        notes.push(
            "a Windows program: it runs through Proton; `%DATA%` in its arguments is the staged Data folder as Wine names it"
                .to_owned(),
        );
        let mut args = vec!["run".to_owned(), binary.to_string_lossy().into_owned()];
        args.extend(tool_args);
        (
            req.proton
                .unwrap_or(nvmod_instance::DEFAULT_PROTON)
                .to_owned(),
            args,
        )
    } else {
        (binary.to_string_lossy().into_owned(), tool_args)
    };
    Ok(RunPlan {
        entry: entry.title.clone(),
        kind,
        command: LaunchSpec {
            program,
            args,
            env,
            working_dir,
            log: None,
        },
        via_proton,
        data_dir: data,
        staging: StagingAction::Current,
        native: None,
        notes,
    })
}

/// Whether a variable name looks like it holds a secret, so its value is never printed.
#[must_use]
pub fn is_secret_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["TOKEN", "KEY", "SECRET", "PASSWORD", "PASSWD", "CREDENTIAL"]
        .iter()
        .any(|w| upper.contains(w))
}

impl RunPlan {
    /// The plan as text for people. Variables nvmod sets are listed; the caller's own environment is passed on
    /// and never printed, and a value whose name looks secret shows as `<hidden>`.
    #[must_use]
    pub fn to_text(&self) -> String {
        let c = &self.command;
        let mut lines = vec![format!("run `{}` ({})", self.entry, self.kind)];
        lines.push(format!(
            "  staging: {}",
            self.staging.describe(&profile_of(self))
        ));
        lines.push(format!("  data: {}", self.data_dir.display()));
        if let Some(n) = &self.native {
            lines.push(format!("  run folder: {}", n.run_dir.display()));
            lines.push(format!("  user data: {}", n.userdata.display()));
            lines.push(format!("  viewer: {}", n.viewer.display()));
            lines.push(format!("  log: {}", n.log.display()));
        }
        lines.push(format!("  working directory: {}", c.working_dir.display()));
        for (k, v) in &c.env {
            let shown = if is_secret_name(k) { "<hidden>" } else { v };
            lines.push(format!("  env: {k}={shown}"));
        }
        let mut cmd = vec![c.program.clone()];
        cmd.extend(c.args.iter().cloned());
        lines.push(format!("  command: {}", cmd.join(" ")));
        lines.push("  the rest of the environment is inherited".to_owned());
        for n in &self.notes {
            lines.push(format!("  note: {n}"));
        }
        lines.join("\n") + "\n"
    }

    /// The plan as JSON for scripts. Secret-looking variable values are replaced by `<hidden>`.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        let mut value = serde_json::to_value(self).unwrap_or_default();
        if let Some(env) = value["command"]["env"].as_object_mut() {
            for (k, v) in env.iter_mut() {
                if is_secret_name(k) {
                    *v = serde_json::Value::String("<hidden>".to_owned());
                }
            }
        }
        value
    }
}

fn profile_of(plan: &RunPlan) -> String {
    plan.command
        .env
        .get("NVMOD_PROFILE")
        .cloned()
        .unwrap_or_default()
}

/// Decide what happens to the Staging tree of `profile` before a run: built when missing, rebuilt when
/// `modlist.txt`, `profile.json`, the `mods` folder or `overwrite` is newer than the tree's manifest or when
/// `force`, kept otherwise, and left alone when `skip`. A change inside a Mod's own folder does not move the
/// `mods` folder's time; `--restage` is for that.
#[must_use]
pub fn plan_staging(
    instance: &NvmodInstance,
    profile: &str,
    force: bool,
    skip: bool,
) -> StagingAction {
    if skip {
        return StagingAction::Skipped;
    }
    let manifest = instance.staging_dir(profile).with_extension("json");
    let tree = data_dir(instance, profile);
    let Some(built) = std::fs::metadata(&manifest)
        .ok()
        .filter(|_| tree.is_dir())
        .and_then(|m| m.modified().ok())
    else {
        return StagingAction::Build;
    };
    if force {
        return StagingAction::Rebuild {
            reason: "--restage".to_owned(),
        };
    }
    let inputs = [
        instance.profile_dir(profile).join("modlist.txt"),
        instance.profile_dir(profile).join("profile.json"),
        instance.root.join("mods"),
        instance.root.join("overwrite"),
    ];
    for input in &inputs {
        if std::fs::metadata(input)
            .and_then(|m| m.modified())
            .is_ok_and(|t| t > built)
        {
            let name = input
                .strip_prefix(&instance.root)
                .unwrap_or(input)
                .display()
                .to_string();
            return StagingAction::Rebuild {
                reason: format!("{name} changed after the tree was built"),
            };
        }
    }
    if nvmod_stage::times_stale(instance, profile) {
        return StagingAction::Rebuild {
            reason: "the plugin file times do not follow loadorder.txt".to_owned(),
        };
    }
    StagingAction::Current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_split_on_spaces_and_group_in_quotes() {
        assert_eq!(
            split_arguments(r#"-D:"Z:\a b" -x  "" y"#),
            ["-D:Z:\\a b", "-x", "", "y"]
        );
        assert_eq!(split_arguments("   "), Vec::<String>::new());
    }

    #[test]
    fn wine_paths_use_z_and_backslashes() {
        assert_eq!(wine_path(Path::new("/home/u/data")), "Z:\\home\\u\\data");
    }

    #[test]
    fn secret_names_are_recognised() {
        assert!(is_secret_name("NEXUS_API_KEY") && is_secret_name("gh_token"));
        assert!(!is_secret_name("NVMOD_DATA_DIR"));
    }
}
