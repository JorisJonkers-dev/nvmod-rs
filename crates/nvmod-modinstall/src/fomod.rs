// SPDX-License-Identifier: GPL-3.0-only
//! FOMOD: `fomod/ModuleConfig.xml` read permissively, and the choices of the installer worked out from it.
//!
//! A group is answered without the owner when the XML leaves no room: `SelectAll`; a `Required` option; one
//! `Recommended` option; or a single usable option where one must be chosen. Every other group becomes a
//! [`Question`], and an answers file completes it. Nothing is guessed: a type that depends on something this
//! program cannot see makes the group a question too.

use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

/// What the Instance says about a plugin, for `fileDependency`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    /// Listed in `plugins.txt`.
    Active,
    /// Listed in `loadorder.txt` only.
    Inactive,
    /// In neither list.
    Missing,
}

/// The Instance as far as FOMOD conditions can see it.
pub trait Env {
    /// The state of `file`, or `None` when it is not a plugin (nothing here can tell).
    fn file_state(&self, file: &str) -> Option<FileState>;
}

/// How a group limits the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GroupKind {
    /// `SelectAny`.
    SelectAny,
    /// `SelectAll`.
    SelectAll,
    /// `SelectExactlyOne`.
    SelectExactlyOne,
    /// `SelectAtMostOne`.
    SelectAtMostOne,
    /// `SelectAtLeastOne`.
    SelectAtLeastOne,
}

impl GroupKind {
    fn parse(text: &str) -> GroupKind {
        match text.trim().to_ascii_lowercase().as_str() {
            "selectall" => GroupKind::SelectAll,
            "selectexactlyone" => GroupKind::SelectExactlyOne,
            "selectatmostone" => GroupKind::SelectAtMostOne,
            "selectatleastone" => GroupKind::SelectAtLeastOne,
            _ => GroupKind::SelectAny,
        }
    }

    /// The name used in the XML and in the questions.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            GroupKind::SelectAny => "SelectAny",
            GroupKind::SelectAll => "SelectAll",
            GroupKind::SelectExactlyOne => "SelectExactlyOne",
            GroupKind::SelectAtMostOne => "SelectAtMostOne",
            GroupKind::SelectAtLeastOne => "SelectAtLeastOne",
        }
    }
}

/// A plugin's type descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PluginType {
    /// Must be installed.
    Required,
    /// Suggested by the author.
    Recommended,
    /// Free to choose.
    Optional,
    /// Cannot be installed.
    NotUsable,
    /// Free to choose, with a warning.
    CouldBeUsable,
}

impl PluginType {
    fn parse(text: &str) -> PluginType {
        match text.trim().to_ascii_lowercase().as_str() {
            "required" => PluginType::Required,
            "recommended" => PluginType::Recommended,
            "notusable" => PluginType::NotUsable,
            "couldbeusable" => PluginType::CouldBeUsable,
            _ => PluginType::Optional,
        }
    }
}

#[derive(Debug, Clone)]
enum Dep {
    Flag(String, String),
    File(String, FileState),
    /// A game or script extender version: assumed satisfied, the Doctor checks versions.
    Version,
    Group(bool, Vec<Dep>),
}

#[derive(Debug, Clone)]
enum TypeSpec {
    Static(PluginType),
    Dynamic(PluginType, Vec<(Dep, PluginType)>),
}

/// A file or folder to install, as the FOMOD names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileItem {
    /// Path in the archive (relative to the folder that holds `fomod/`).
    pub source: String,
    /// Path in the Mod folder; empty means the root.
    pub destination: String,
    /// A folder, installed with its contents.
    pub folder: bool,
    /// Higher wins when two items write the same file.
    pub priority: i32,
}

#[derive(Debug, Clone)]
struct Plugin {
    name: String,
    description: String,
    files: Vec<FileItem>,
    flags: Vec<(String, String)>,
    kind: TypeSpec,
}

#[derive(Debug, Clone)]
struct Group {
    name: String,
    kind: GroupKind,
    plugins: Vec<Plugin>,
}

#[derive(Debug, Clone)]
struct Step {
    name: String,
    visible: Option<Dep>,
    groups: Vec<Group>,
}

#[derive(Debug, Clone)]
struct Pattern {
    dep: Dep,
    files: Vec<FileItem>,
}

/// A parsed `ModuleConfig.xml`.
#[derive(Debug, Clone)]
pub struct Module {
    required: Vec<FileItem>,
    steps: Vec<Step>,
    patterns: Vec<Pattern>,
}

type Node<'a, 'b> = roxmltree::Node<'a, 'b>;

fn children<'a, 'b>(node: Node<'a, 'b>, tag: &'a str) -> impl Iterator<Item = Node<'a, 'b>> {
    node.children()
        .filter(move |n| n.is_element() && n.tag_name().name().eq_ignore_ascii_case(tag))
}

fn child<'a, 'b>(node: Node<'a, 'b>, tag: &'a str) -> Option<Node<'a, 'b>> {
    children(node, tag).next()
}

fn attr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|a| a.name().eq_ignore_ascii_case(name))
        .map(|a| a.value())
}

/// Children in the order the `order` attribute of `container` asks for (the schema's default is ascending by
/// name).
fn ordered<'a, 'b>(container: Option<Node<'a, 'b>>, tag: &'a str) -> Vec<Node<'a, 'b>> {
    let Some(container) = container else {
        return Vec::new();
    };
    let mut items: Vec<_> = children(container, tag).collect();
    let order = attr(container, "order").unwrap_or("Ascending");
    let key = |n: &Node| attr(*n, "name").unwrap_or_default().to_lowercase();
    if order.eq_ignore_ascii_case("Ascending") {
        items.sort_by_key(key);
    } else if order.eq_ignore_ascii_case("Descending") {
        items.sort_by_key(key);
        items.reverse();
    }
    items
}

fn files_of(node: Option<Node>) -> Vec<FileItem> {
    let Some(node) = node else {
        return Vec::new();
    };
    node.children()
        .filter(Node::is_element)
        .filter_map(|n| {
            let tag = n.tag_name().name().to_ascii_lowercase();
            if tag != "file" && tag != "folder" {
                return None;
            }
            Some(FileItem {
                source: attr(n, "source")?.to_owned(),
                destination: attr(n, "destination").unwrap_or_default().to_owned(),
                folder: tag == "folder",
                priority: attr(n, "priority")
                    .and_then(|p| p.trim().parse().ok())
                    .unwrap_or(0),
            })
        })
        .collect()
}

fn dep_of(node: Node) -> Dep {
    let tag = node.tag_name().name().to_ascii_lowercase();
    match tag.as_str() {
        "flagdependency" => Dep::Flag(
            attr(node, "flag").unwrap_or_default().to_owned(),
            attr(node, "value").unwrap_or_default().to_owned(),
        ),
        "filedependency" => {
            let state = match attr(node, "state")
                .unwrap_or("Active")
                .to_ascii_lowercase()
                .as_str()
            {
                "inactive" => FileState::Inactive,
                "missing" => FileState::Missing,
                _ => FileState::Active,
            };
            Dep::File(attr(node, "file").unwrap_or_default().to_owned(), state)
        }
        "gamedependency" | "fosedependency" => Dep::Version,
        _ => {
            let and = !attr(node, "operator").is_some_and(|o| o.eq_ignore_ascii_case("Or"));
            Dep::Group(
                and,
                node.children()
                    .filter(Node::is_element)
                    .map(dep_of)
                    .collect(),
            )
        }
    }
}

fn type_of(node: Option<Node>) -> TypeSpec {
    let Some(node) = node else {
        return TypeSpec::Static(PluginType::Optional);
    };
    if let Some(dynamic) = child(node, "dependencyType") {
        let default = child(dynamic, "defaultType")
            .and_then(|d| attr(d, "name"))
            .map_or(PluginType::Optional, PluginType::parse);
        let patterns = child(dynamic, "patterns")
            .into_iter()
            .flat_map(|p| children(p, "pattern"))
            .map(|p| {
                let dep = child(p, "dependencies").map_or(Dep::Group(true, Vec::new()), dep_of);
                let kind = child(p, "type")
                    .and_then(|t| attr(t, "name"))
                    .map_or(PluginType::Optional, PluginType::parse);
                (dep, kind)
            })
            .collect();
        return TypeSpec::Dynamic(default, patterns);
    }
    TypeSpec::Static(
        child(node, "type")
            .and_then(|t| attr(t, "name"))
            .map_or(PluginType::Optional, PluginType::parse),
    )
}

/// Decode the bytes of a `ModuleConfig.xml`: UTF-16 or UTF-8 with a byte order mark, UTF-8, else Windows-1252.
#[must_use]
pub fn decode_xml(bytes: &[u8]) -> String {
    let utf16 = |rest: &[u8], le: bool| {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| {
                if le {
                    u16::from_le_bytes(*p)
                } else {
                    u16::from_be_bytes(*p)
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        utf16(rest, true)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        utf16(rest, false)
    } else {
        nvmod_formats::text::decode(bytes)
    }
}

impl Module {
    /// Parse the text of a `ModuleConfig.xml`.
    ///
    /// # Errors
    /// The text is not well-formed XML, or has no `config` root.
    pub fn parse(text: &str) -> Result<Module, String> {
        let options = roxmltree::ParsingOptions {
            allow_dtd: true,
            ..roxmltree::ParsingOptions::default()
        };
        let doc = roxmltree::Document::parse_with_options(text, options)
            .map_err(|e| format!("ModuleConfig.xml is not valid XML: {e}"))?;
        let root = doc.root_element();
        if !root.tag_name().name().eq_ignore_ascii_case("config") {
            return Err("ModuleConfig.xml has no <config> root".to_owned());
        }
        let steps = ordered(child(root, "installSteps"), "installStep")
            .into_iter()
            .map(|s| Step {
                name: attr(s, "name").unwrap_or_default().to_owned(),
                visible: child(s, "visible")
                    .and_then(|v| child(v, "dependencies"))
                    .map(dep_of),
                groups: ordered(child(s, "optionalFileGroups"), "group")
                    .into_iter()
                    .map(|g| Group {
                        name: attr(g, "name").unwrap_or_default().to_owned(),
                        kind: GroupKind::parse(attr(g, "type").unwrap_or_default()),
                        plugins: ordered(child(g, "plugins"), "plugin")
                            .into_iter()
                            .map(|p| Plugin {
                                name: attr(p, "name").unwrap_or_default().to_owned(),
                                description: child(p, "description")
                                    .and_then(|d| d.text())
                                    .unwrap_or_default()
                                    .trim()
                                    .to_owned(),
                                files: files_of(child(p, "files")),
                                flags: child(p, "conditionFlags")
                                    .into_iter()
                                    .flat_map(|c| children(c, "flag"))
                                    .map(|f| {
                                        (
                                            attr(f, "name").unwrap_or_default().to_owned(),
                                            f.text().unwrap_or_default().trim().to_owned(),
                                        )
                                    })
                                    .collect(),
                                kind: type_of(child(p, "typeDescriptor")),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect();
        let patterns = child(root, "conditionalFileInstalls")
            .and_then(|c| child(c, "patterns"))
            .into_iter()
            .flat_map(|p| children(p, "pattern"))
            .map(|p| Pattern {
                dep: child(p, "dependencies").map_or(Dep::Group(true, Vec::new()), dep_of),
                files: files_of(child(p, "files")),
            })
            .collect();
        Ok(Module {
            required: files_of(child(root, "requiredInstallFiles")),
            steps,
            patterns,
        })
    }
}

/// An answer to every question in one go: question id to the names of the chosen options.
pub type Answers = BTreeMap<String, Vec<String>>;

/// One option of a [`Question`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuestionOption {
    /// The name to give back in the answers file.
    pub name: String,
    /// The author's description.
    pub description: String,
    /// The type, when it is known (`None`: it depends on something unknown).
    #[serde(rename = "type")]
    pub kind: Option<PluginType>,
}

/// A group the installer cannot answer by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Question {
    /// `step name/group name`, the key in the answers file.
    pub id: String,
    /// The install step.
    pub step: String,
    /// The group.
    pub group: String,
    /// The group's rule.
    pub kind: GroupKind,
    /// The fewest options to choose.
    pub min: usize,
    /// The most options to choose.
    pub max: usize,
    /// The choices.
    pub options: Vec<QuestionOption>,
    /// True when an earlier unanswered question decides whether this step is shown at all.
    pub may_not_apply: bool,
}

/// A choice made without asking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Inferred {
    /// The question id it would have had.
    pub id: String,
    /// The options chosen.
    pub selected: Vec<String>,
    /// Why this was unambiguous.
    pub reason: String,
}

/// The files a FOMOD installs, in install order, and how they were chosen.
#[derive(Debug, Clone)]
pub struct Selection {
    /// Items in the order they are installed (a later one wins on equal priority).
    pub files: Vec<FileItem>,
    /// Choices made without asking.
    pub inferred: Vec<Inferred>,
}

/// What the module needs to go on.
#[derive(Debug)]
pub enum Outcome {
    /// Everything is decided.
    Ready(Selection),
    /// These groups need an answer.
    Questions(Vec<Question>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tri {
    Yes,
    No,
    Unknown,
}

fn and(a: Tri, b: Tri) -> Tri {
    match (a, b) {
        (Tri::No, _) | (_, Tri::No) => Tri::No,
        (Tri::Yes, Tri::Yes) => Tri::Yes,
        _ => Tri::Unknown,
    }
}

fn or(a: Tri, b: Tri) -> Tri {
    match (a, b) {
        (Tri::Yes, _) | (_, Tri::Yes) => Tri::Yes,
        (Tri::No, Tri::No) => Tri::No,
        _ => Tri::Unknown,
    }
}

struct State<'a> {
    env: &'a dyn Env,
    flags: HashMap<String, String>,
    /// Flags that an unanswered question may still set.
    maybe: HashSet<String>,
}

impl State<'_> {
    fn eval(&self, dep: &Dep) -> Tri {
        match dep {
            Dep::Version => Tri::Yes,
            Dep::Flag(name, value) => {
                let hit = self.flags.get(name).is_some_and(|v| v == value);
                if hit {
                    Tri::Yes
                } else if self.maybe.contains(name) {
                    Tri::Unknown
                } else {
                    Tri::No
                }
            }
            Dep::File(file, wanted) => match self.env.file_state(file) {
                Some(state) if state == *wanted => Tri::Yes,
                Some(_) => Tri::No,
                None => Tri::Unknown,
            },
            Dep::Group(true, deps) => deps.iter().fold(Tri::Yes, |acc, d| and(acc, self.eval(d))),
            Dep::Group(false, deps) => {
                if deps.is_empty() {
                    Tri::Yes
                } else {
                    deps.iter().fold(Tri::No, |acc, d| or(acc, self.eval(d)))
                }
            }
        }
    }

    fn type_of(&self, plugin: &Plugin) -> Option<PluginType> {
        match &plugin.kind {
            TypeSpec::Static(t) => Some(*t),
            TypeSpec::Dynamic(default, patterns) => {
                for (dep, kind) in patterns {
                    match self.eval(dep) {
                        Tri::Yes => return Some(*kind),
                        Tri::No => {}
                        Tri::Unknown => return None,
                    }
                }
                Some(*default)
            }
        }
    }
}

/// The picks of a group that need no question, with the reason; `None` when the group is ambiguous.
fn infer(kind: GroupKind, types: &[Option<PluginType>]) -> Option<(Vec<usize>, &'static str)> {
    if kind == GroupKind::SelectAll {
        return Some(((0..types.len()).collect(), "the group is SelectAll"));
    }
    let types: Vec<PluginType> = types.iter().copied().collect::<Option<_>>()?;
    let usable: Vec<usize> = (0..types.len())
        .filter(|&i| types[i] != PluginType::NotUsable)
        .collect();
    let of = |t: PluginType| -> Vec<usize> {
        usable.iter().copied().filter(|&i| types[i] == t).collect()
    };
    let (required, recommended) = (of(PluginType::Required), of(PluginType::Recommended));
    let strong: Vec<usize> = usable
        .iter()
        .copied()
        .filter(|&i| matches!(types[i], PluginType::Required | PluginType::Recommended))
        .collect();
    let by_type = || {
        if required.len() == 1 {
            Some((required.clone(), "the author marks it Required"))
        } else if required.is_empty() && recommended.len() == 1 {
            Some((recommended.clone(), "the author marks it Recommended"))
        } else {
            None
        }
    };
    match kind {
        GroupKind::SelectAll => None,
        GroupKind::SelectExactlyOne => by_type().or_else(|| {
            (usable.len() == 1 && required.is_empty())
                .then(|| (usable.clone(), "it is the only usable option"))
        }),
        GroupKind::SelectAtMostOne => {
            if usable.is_empty() {
                Some((Vec::new(), "no option can be used"))
            } else {
                by_type()
            }
        }
        GroupKind::SelectAtLeastOne => {
            if strong.is_empty() {
                (usable.len() == 1).then(|| (usable.clone(), "it is the only usable option"))
            } else {
                Some((strong, "the Required and Recommended options"))
            }
        }
        GroupKind::SelectAny => {
            if usable.is_empty() {
                Some((Vec::new(), "no option can be used"))
            } else if strong.is_empty() {
                None
            } else {
                Some((strong, "the Required and Recommended options"))
            }
        }
    }
}

fn bounds(kind: GroupKind, options: usize) -> (usize, usize) {
    match kind {
        GroupKind::SelectAll => (options, options),
        GroupKind::SelectExactlyOne => (1, 1),
        GroupKind::SelectAtMostOne => (0, 1),
        GroupKind::SelectAtLeastOne => (1, options),
        GroupKind::SelectAny => (0, options),
    }
}

impl Module {
    /// The ids of every group, in order (a repeated `step/group` gets ` #2`, ` #3`, ...).
    fn ids(&self) -> Vec<Vec<String>> {
        let mut seen: HashMap<String, usize> = HashMap::new();
        self.steps
            .iter()
            .map(|s| {
                s.groups
                    .iter()
                    .map(|g| {
                        let base = format!("{}/{}", s.name, g.name);
                        let n = seen.entry(base.clone()).or_insert(0);
                        *n += 1;
                        if *n == 1 {
                            base
                        } else {
                            format!("{base} #{n}")
                        }
                    })
                    .collect()
            })
            .collect()
    }

    /// Work out what to install, given the answers so far.
    ///
    /// # Errors
    /// An answer names a group or option that does not exist, breaks the group's rule, or picks an option that
    /// cannot be used; or a group offers nothing to choose from.
    pub fn resolve(&self, env: &dyn Env, answers: &Answers) -> Result<Outcome, String> {
        let ids = self.ids();
        let known: HashSet<&String> = ids.iter().flatten().collect();
        if let Some(bad) = answers.keys().find(|k| !known.contains(k)) {
            return Err(format!(
                "the answers file answers `{bad}`, which the FOMOD does not ask"
            ));
        }
        let mut st = State {
            env,
            flags: HashMap::new(),
            maybe: HashSet::new(),
        };
        let mut files = self.required.clone();
        let (mut inferred, mut questions) = (Vec::new(), Vec::new());
        for (si, step) in self.steps.iter().enumerate() {
            let visible = step.visible.as_ref().map_or(Tri::Yes, |d| st.eval(d));
            if visible == Tri::No {
                continue;
            }
            for (gi, group) in step.groups.iter().enumerate() {
                let id = ids[si][gi].clone();
                let types: Vec<Option<PluginType>> =
                    group.plugins.iter().map(|p| st.type_of(p)).collect();
                let picked = if let Some(given) = answers.get(&id) {
                    Some((check_answer(group, &types, given, &id)?, None))
                } else {
                    infer(group.kind, &types).map(|(p, why)| (p, Some(why)))
                };
                let Some((picks, why)) = picked else {
                    if group
                        .plugins
                        .iter()
                        .all(|p| matches!(&p.kind, TypeSpec::Static(PluginType::NotUsable)))
                    {
                        return Err(format!("`{id}` offers no option that can be used"));
                    }
                    let (min, max) = bounds(group.kind, group.plugins.len());
                    questions.push(Question {
                        id,
                        step: step.name.clone(),
                        group: group.name.clone(),
                        kind: group.kind,
                        min,
                        max,
                        options: group
                            .plugins
                            .iter()
                            .zip(&types)
                            .map(|(p, t)| QuestionOption {
                                name: p.name.clone(),
                                description: p.description.clone(),
                                kind: *t,
                            })
                            .collect(),
                        may_not_apply: visible == Tri::Unknown,
                    });
                    for plugin in &group.plugins {
                        st.maybe.extend(plugin.flags.iter().map(|(n, _)| n.clone()));
                    }
                    continue;
                };
                if let Some(why) = why {
                    inferred.push(Inferred {
                        id,
                        selected: picks
                            .iter()
                            .map(|&i| group.plugins[i].name.clone())
                            .collect(),
                        reason: why.to_owned(),
                    });
                }
                for &i in &picks {
                    let plugin = &group.plugins[i];
                    if visible == Tri::Yes {
                        files.extend(plugin.files.iter().cloned());
                        st.flags.extend(plugin.flags.iter().cloned());
                    } else {
                        st.maybe.extend(plugin.flags.iter().map(|(n, _)| n.clone()));
                    }
                }
            }
        }
        if !questions.is_empty() {
            return Ok(Outcome::Questions(questions));
        }
        for pattern in &self.patterns {
            if st.eval(&pattern.dep) == Tri::Yes {
                files.extend(pattern.files.iter().cloned());
            }
        }
        Ok(Outcome::Ready(Selection { files, inferred }))
    }
}

fn check_answer(
    group: &Group,
    types: &[Option<PluginType>],
    given: &[String],
    id: &str,
) -> Result<Vec<usize>, String> {
    let mut picks = Vec::new();
    for name in given {
        let index = group
            .plugins
            .iter()
            .position(|p| &p.name == name)
            .ok_or_else(|| format!("`{id}` has no option `{name}`"))?;
        if !picks.contains(&index) {
            picks.push(index);
        }
    }
    picks.sort_unstable();
    let (min, max) = bounds(group.kind, group.plugins.len());
    if picks.len() < min || picks.len() > max {
        return Err(format!(
            "`{id}` is {} and takes {min} to {max} option(s); the answer has {}",
            group.kind.as_str(),
            picks.len()
        ));
    }
    for (i, t) in types.iter().enumerate() {
        match t {
            Some(PluginType::NotUsable) if picks.contains(&i) => {
                return Err(format!(
                    "`{id}`: option `{}` cannot be used",
                    group.plugins[i].name
                ));
            }
            Some(PluginType::Required) if !picks.contains(&i) => {
                return Err(format!(
                    "`{id}`: option `{}` is Required",
                    group.plugins[i].name
                ));
            }
            _ => {}
        }
    }
    Ok(picks)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Plugins(Vec<(&'static str, FileState)>);

    impl Env for Plugins {
        fn file_state(&self, file: &str) -> Option<FileState> {
            if !file.to_ascii_lowercase().ends_with(".esp") {
                return None;
            }
            Some(
                self.0
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(file))
                    .map_or(FileState::Missing, |(_, s)| *s),
            )
        }
    }

    fn group(kind: &str, plugins: &str) -> String {
        format!(
            r#"<config><installSteps order="Explicit"><installStep name="S"><optionalFileGroups order="Explicit">
            <group name="G" type="{kind}"><plugins order="Explicit">{plugins}</plugins></group>
            </optionalFileGroups></installStep></installSteps></config>"#
        )
    }

    fn option(name: &str, kind: &str) -> String {
        format!(
            r#"<plugin name="{name}"><files><file source="{name}" destination="{name}"/></files>
            <typeDescriptor><type name="{kind}"/></typeDescriptor></plugin>"#
        )
    }

    fn picks(xml: &str, env: &dyn Env, answers: &Answers) -> Result<Vec<String>, String> {
        match Module::parse(xml)?.resolve(env, answers)? {
            Outcome::Ready(s) => Ok(s.files.into_iter().map(|f| f.source).collect()),
            Outcome::Questions(q) => Err(format!("questions: {}", q[0].id)),
        }
    }

    fn none() -> Plugins {
        Plugins(Vec::new())
    }

    #[test]
    fn each_group_kind_infers_only_what_is_unambiguous() {
        let two = |a: &str, b: &str| option("A", a) + &option("B", b);
        let cases = [
            (
                "SelectExactlyOne",
                two("Required", "Optional"),
                Ok(vec!["A"]),
            ),
            (
                "SelectExactlyOne",
                two("Optional", "Recommended"),
                Ok(vec!["B"]),
            ),
            (
                "SelectExactlyOne",
                two("Optional", "NotUsable"),
                Ok(vec!["A"]),
            ),
            ("SelectExactlyOne", two("Optional", "Optional"), Err(())),
            (
                "SelectExactlyOne",
                two("Recommended", "Recommended"),
                Err(()),
            ),
            (
                "SelectAtMostOne",
                two("Optional", "Recommended"),
                Ok(vec!["B"]),
            ),
            ("SelectAtMostOne", option("A", "Optional"), Err(())),
            ("SelectAtMostOne", two("NotUsable", "NotUsable"), Ok(vec![])),
            (
                "SelectAtLeastOne",
                two("Required", "Recommended"),
                Ok(vec!["A", "B"]),
            ),
            ("SelectAtLeastOne", option("A", "Optional"), Ok(vec!["A"])),
            ("SelectAtLeastOne", two("Optional", "Optional"), Err(())),
            ("SelectAny", two("Recommended", "Optional"), Ok(vec!["A"])),
            ("SelectAny", two("Optional", "CouldBeUsable"), Err(())),
            (
                "SelectAll",
                two("Optional", "NotUsable"),
                Ok(vec!["A", "B"]),
            ),
        ];
        for (kind, plugins, want) in cases {
            let got = picks(&group(kind, &plugins), &none(), &Answers::new());
            match want {
                Ok(files) => assert_eq!(
                    got,
                    Ok(files.into_iter().map(str::to_owned).collect()),
                    "{kind}"
                ),
                Err(()) => assert!(got.is_err_and(|e| e.starts_with("questions")), "{kind}"),
            }
        }
    }

    #[test]
    fn a_type_that_depends_on_a_plugin_is_read_from_the_lists_and_unknown_files_are_asked() {
        let dynamic = |file: &str| {
            format!(
                r#"<plugin name="A"><files><file source="A" destination="A"/></files><typeDescriptor><dependencyType>
                <defaultType name="Optional"/><patterns><pattern><dependencies><fileDependency file="{file}" state="Active"/></dependencies>
                <type name="Recommended"/></pattern></patterns></dependencyType></typeDescriptor></plugin>{}"#,
                option("B", "Optional")
            )
        };
        let xml = group("SelectExactlyOne", &dynamic("Patch.esp"));
        let active = Plugins(vec![("patch.esp", FileState::Active)]);
        assert_eq!(
            picks(&xml, &active, &Answers::new()),
            Ok(vec!["A".to_owned()])
        );
        let inactive = Plugins(vec![("patch.esp", FileState::Inactive)]);
        assert!(
            picks(&xml, &inactive, &Answers::new()).is_err(),
            "default Optional: two options, ask"
        );
        let unknown = group("SelectExactlyOne", &dynamic("nvse_loader.exe"));
        assert!(
            picks(&unknown, &active, &Answers::new()).is_err(),
            "not a plugin: cannot tell"
        );
    }

    #[test]
    fn answers_are_checked_against_the_group_and_the_types() {
        let plugins =
            option("A", "NotUsable") + &option("B", "Optional") + &option("C", "Optional");
        let xml = group("SelectExactlyOne", &plugins);
        let answer = |names: &[&str]| -> Answers {
            Answers::from([(
                "S/G".to_owned(),
                names.iter().map(|n| (*n).to_owned()).collect(),
            )])
        };
        assert_eq!(
            picks(&xml, &none(), &answer(&["C"])),
            Ok(vec!["C".to_owned()])
        );
        for bad in [&["A"][..], &["B", "C"], &[], &["Z"]] {
            assert!(
                picks(&xml, &none(), &answer(bad)).is_err_and(|e| !e.starts_with("questions")),
                "{bad:?}"
            );
        }
        let unknown = Answers::from([("S/Nope".to_owned(), vec![])]);
        assert!(picks(&xml, &none(), &unknown).is_err_and(|e| e.contains("does not ask")));
    }

    #[test]
    fn a_step_shown_only_by_a_flag_is_skipped_or_asked_about() {
        let xml = r#"<config><installSteps order="Explicit">
          <installStep name="One"><optionalFileGroups><group name="Pick" type="SelectExactlyOne"><plugins order="Explicit">
            <plugin name="On"><conditionFlags><flag name="x">1</flag></conditionFlags><files><file source="on" destination="on"/></files>
              <typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
            <plugin name="Off"><files><file source="off" destination="off"/></files>
              <typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
          </plugins></group></optionalFileGroups></installStep>
          <installStep name="Two"><visible><dependencies><flagDependency flag="x" value="1"/></dependencies></visible>
            <optionalFileGroups><group name="More" type="SelectAll"><plugins>
              <plugin name="M"><files><file source="more" destination="more"/></files></plugin>
            </plugins></group></optionalFileGroups></installStep>
        </installSteps></config>"#;
        let m = Module::parse(xml).unwrap();
        let Outcome::Questions(q) = m.resolve(&none(), &Answers::new()).unwrap() else {
            panic!("expected a question");
        };
        assert_eq!(
            q.len(),
            1,
            "the SelectAll group of the second step is inferable"
        );
        let on = Answers::from([("One/Pick".to_owned(), vec!["On".to_owned()])]);
        assert_eq!(
            picks(xml, &none(), &on),
            Ok(vec!["on".to_owned(), "more".to_owned()])
        );
        let off = Answers::from([("One/Pick".to_owned(), vec!["Off".to_owned()])]);
        assert_eq!(picks(xml, &none(), &off), Ok(vec!["off".to_owned()]));
    }

    #[test]
    fn xml_in_utf16_or_with_a_byte_order_mark_is_read() {
        let xml = "<config><moduleName>M\u{e9}</moduleName></config>";
        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(decode_xml(&utf16), xml);
        assert_eq!(decode_xml(format!("\u{feff}{xml}").as_bytes()), xml);
        assert!(Module::parse("<notconfig/>").is_err());
        assert!(Module::parse("<config>").is_err());
    }
}
