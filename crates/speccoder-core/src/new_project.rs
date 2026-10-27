//! A new project's plan: the exact commands that make an empty folder (or
//! a clean repository) a project speccoder can judge — the stack's own
//! scaffold, the spec format's own init, `speccoder init --yes` and one
//! commit as the person — with what each touches, whether it reaches the
//! network, and the folder as it was seen. The person confirms this plan;
//! the runtime runs exactly it, bound to its digest.

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

/// A stack the plan can scaffold, or none.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Stack {
    /// Rust with cargo.
    Rust,
    /// Python with uv and pytest.
    Python,
    /// TypeScript with npm and vitest.
    Node,
    /// Go modules.
    Go,
    /// No scaffold: the person adds code.
    None,
}

impl Stack {
    /// Every stack, in the sheet's order.
    pub const ALL: [Self; 5] = [Self::Rust, Self::Python, Self::Node, Self::Go, Self::None];

    /// Its name as serialized (`node`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::Node => "node",
            Self::Go => "go",
            Self::None => "none",
        }
    }

    /// How the sheet names it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rust => "Rust (cargo)",
            Self::Python => "Python (uv)",
            Self::Node => "TypeScript (npm + vitest)",
            Self::Go => "Go",
            Self::None => "I'll add code myself",
        }
    }

    /// The tool its scaffold runs (none for [`Stack::None`]).
    #[must_use]
    pub const fn tool(self) -> Option<&'static str> {
        match self {
            Self::Rust => Some("cargo"),
            Self::Python => Some("uv"),
            Self::Node => Some("npm"),
            Self::Go => Some("go"),
            Self::None => None,
        }
    }

    /// Where to install its tool.
    #[must_use]
    pub const fn install_hint(self) -> &'static str {
        match self {
            Self::Rust => "https://rustup.rs",
            Self::Python => "https://docs.astral.sh/uv/",
            Self::Node => "https://nodejs.org",
            Self::Go => "https://go.dev/dl/",
            Self::None => "",
        }
    }

    /// The stack named `s` (`rust`, `python`, `node`, `go`, `none`).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// Whether a step stays on the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepAccess {
    /// No network.
    Local,
    /// It reaches the hosts it names.
    Network,
}

/// Where the project is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectMode {
    /// A folder that does not exist yet, or an empty one.
    NewFolder,
    /// A git repository at its top level with a clean tree and no spec project.
    ExistingRepository,
}

/// A step's state in a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanStepState {
    /// Not started.
    Waiting,
    /// Running now.
    Running,
    /// Finished.
    Done,
    /// Its effect was already there (resume).
    Skipped,
    /// Failed; later steps did not run.
    Failed,
}

impl PlanStepState {
    /// The word a person reads.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Running => "running",
            Self::Done => "done",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }
}

/// A Spec Kit extension the person chose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExtensionChoice {
    /// A bundled name, a local path or a URL, as given.
    pub source: String,
    /// `bundled`, `path` or `url`.
    pub kind: ExtensionKind,
}

/// What an extension source is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionKind {
    /// Bundled with the format's tool (`bug`).
    Bundled,
    /// A local directory.
    Path,
    /// Downloaded: a network step.
    Url,
}

impl ExtensionChoice {
    /// `source` classified: a URL, a path (has a `/` or starts with `.`
    /// or `~`), else a bundled name.
    #[must_use]
    pub fn of(source: &str) -> Self {
        let s = source.trim();
        let kind = if s.starts_with("https://") || s.starts_with("http://") {
            ExtensionKind::Url
        } else if s.contains('/') || s.starts_with('.') || s.starts_with('~') {
            ExtensionKind::Path
        } else {
            ExtensionKind::Bundled
        };
        Self {
            source: s.to_owned(),
            kind,
        }
    }

    /// The host a URL extension reaches.
    #[must_use]
    pub fn host(&self) -> Option<String> {
        if self.kind != ExtensionKind::Url {
            return None;
        }
        let rest = self.source.split_once("://").map(|(_, r)| r)?;
        let host = rest.split(['/', '?', '#']).next()?;
        let host = host.rsplit('@').next()?.split(':').next()?;
        (!host.is_empty()).then(|| host.to_owned())
    }
}

/// What the person chose; with the folder, everything the plan follows from.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NewChoices {
    /// What they are building, verbatim (kept for the first feature).
    pub idea: String,
    /// The stack; `None`: the suggestion.
    pub stack: Option<Stack>,
    /// The spec format (`speckit`, `openspec`); `None`: Spec Kit.
    pub format: Option<String>,
    /// Spec Kit extensions.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Leave out the speccoder Spec Kit preset.
    #[serde(default)]
    pub no_preset: bool,
}

/// One command of the plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanStep {
    /// `git_init`, `scaffold.<n>`, `format_init`, `speccoder_init`, `commit`.
    pub id: String,
    /// The command, exactly as it runs.
    pub argv: Vec<String>,
    /// Where it runs: always the target.
    pub cwd: String,
    /// Local, or network.
    pub access: StepAccess,
    /// The hosts a network step reaches.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<String>,
    /// What it writes, in one line.
    pub writes: String,
    /// Its time limit, in seconds.
    pub limit_s: u32,
    /// A note the plan shows beside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The time limit of a local step, seconds.
pub const LOCAL_LIMIT_S: u32 = 120;
/// The time limit of a network step, seconds.
pub const NETWORK_LIMIT_S: u32 = 600;

/// How a stack was chosen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StackPick {
    /// The stack.
    pub id: Stack,
    /// Found in an existing repository rather than chosen.
    pub detected: bool,
    /// Why it was suggested or detected.
    pub reason: String,
}

/// A stack's tool on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StackOption {
    /// The stack.
    pub id: Stack,
    /// How the sheet names it.
    pub label: String,
    /// The tool's version when installed.
    pub version: Option<String>,
    /// Why it cannot be chosen (its tool is missing), with the install hint.
    pub disabled: Option<String>,
}

/// A spec format the plan can initialise, and its tool's state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FormatOption {
    /// `speckit`, `openspec`.
    pub id: String,
    /// `Spec Kit`, `OpenSpec`.
    pub label: String,
    /// The tool's version when installed.
    pub version: Option<String>,
    /// Why it cannot be chosen.
    pub disabled: Option<String>,
}

/// The person's git identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GitIdentity {
    /// `user.name`.
    pub name: String,
    /// `user.email`.
    pub email: String,
}

/// Why the plan cannot run, and the fix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanRefusal {
    /// `NEW-TARGET-NOT-EMPTY`, …
    pub code: String,
    /// What is wrong.
    pub message: String,
    /// What to do.
    pub fix: String,
}

/// The folder and the tools as the plan saw them.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ObservedFolder {
    /// The folder exists.
    pub exists: bool,
    /// Its entries' names, sorted.
    pub entries: Vec<String>,
    /// `HEAD`, in a repository with a commit.
    pub head: Option<String>,
    /// `git status --porcelain` lines.
    pub porcelain: Vec<String>,
    /// Each tool's version line, by name.
    pub tools: std::collections::BTreeMap<String, String>,
}

/// The OS sandbox the steps run in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SandboxNote {
    /// `seatbelt`, `landlock`; `null`: the commands run without one.
    pub backend: Option<String>,
}

/// The plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectPlan {
    /// The folder, canonical and absolute.
    pub target: String,
    /// The project's name (the folder's).
    pub name: String,
    /// A new folder or an existing repository.
    pub mode: ProjectMode,
    /// The stack.
    pub stack: StackPick,
    /// Every stack and whether its tool is installed.
    pub stacks: Vec<StackOption>,
    /// The spec format (`speckit`, `openspec`).
    pub format: String,
    /// Every format and whether its tool is installed.
    pub formats: Vec<FormatOption>,
    /// Spec Kit extensions.
    pub extensions: Vec<ExtensionChoice>,
    /// Whether the speccoder Spec Kit preset is passed to the format's init.
    pub preset: bool,
    /// The commands, in order.
    pub steps: Vec<PlanStep>,
    /// The OS sandbox.
    pub sandbox: SandboxNote,
    /// The person's git identity.
    pub identity: Option<GitIdentity>,
    /// Why it cannot run (Create disabled).
    pub refusal: Option<PlanRefusal>,
    /// The folder as seen.
    pub observed: ObservedFolder,
    /// SHA-256 over all of the above.
    pub digest: String,
}

impl ProjectPlan {
    /// The hosts its network steps reach, in order, once each.
    #[must_use]
    pub fn network_hosts(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for h in self
            .steps
            .iter()
            .filter(|s| s.access == StepAccess::Network)
            .flat_map(|s| s.hosts.iter())
        {
            if !out.contains(h) {
                out.push(h.clone());
            }
        }
        out
    }
}

/// One step's outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanStepResult {
    /// The step.
    pub id: String,
    /// What happened.
    pub state: PlanStepState,
    /// Its exit status, when it ran.
    pub exit: Option<i32>,
    /// How long it took.
    pub duration_ms: u64,
    /// The last lines of its output (at most 20).
    pub output_tail: Vec<String>,
}

/// What Undo would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UndoOffer {
    /// It can be undone now.
    pub available: bool,
    /// `trash`, `revert` or `none`.
    pub kind: String,
    /// Why not, when not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The `new_project` payload.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct NewProjectReport {
    /// The plan.
    pub plan: ProjectPlan,
    /// Each step's outcome (empty on a dry run).
    pub results: Vec<PlanStepResult>,
    /// `speccoder init`'s report, when it ran.
    pub init: Option<crate::InitReport>,
    /// speccoder's commit, once made.
    pub commit: Option<String>,
    /// What Undo would do.
    pub undo: UndoOffer,
    /// Nothing ran.
    pub dry_run: bool,
    /// The project's first feature, prefilled from the idea.
    pub first_feature: Option<String>,
}

/// The ASCII fold of `text`: decomposed, marks dropped, non-ASCII gone.
#[must_use]
pub fn ascii_fold(text: &str) -> String {
    text.nfkd().filter(char::is_ascii).collect()
}

/// Words a folder name skips.
const FILLER: &[&str] = &[
    "a", "an", "the", "that", "which", "who", "to", "of", "for", "and", "or", "in", "on", "with",
    "my", "our", "your", "into", "from", "by", "at", "as", "is", "it", "its", "this", "i", "we",
    "want", "build", "make", "write", "create", "app", "tool", "simple", "small", "little",
];

/// The longest folder name.
pub const SLUG_MAX: usize = 40;

/// The folder name for an idea: the first five significant words,
/// ASCII-folded, lowercase `[a-z0-9-]`, at most [`SLUG_MAX`] characters;
/// `project` when nothing is left.
#[must_use]
pub fn slug(idea: &str) -> String {
    let folded = ascii_fold(idea).to_ascii_lowercase();
    let words: Vec<&str> = folded
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty() && !FILLER.contains(w))
        .take(5)
        .collect();
    let mut out = String::new();
    for w in words {
        let next = if out.is_empty() {
            w.to_owned()
        } else {
            format!("{out}-{w}")
        };
        if next.len() > SLUG_MAX {
            if out.is_empty() {
                out = w.chars().take(SLUG_MAX).collect();
            }
            break;
        }
        out = next;
    }
    if out.is_empty() {
        "project".to_owned()
    } else {
        out
    }
}

/// Words in an idea that name a stack.
const CUES: &[(Stack, &[&str])] = &[
    (
        Stack::Rust,
        &[
            "rust",
            "cargo",
            "crate",
            "cli",
            "command-line",
            "terminal",
            "library",
            "wasm",
        ],
    ),
    (
        Stack::Python,
        &[
            "python", "pip", "uv", "django", "flask", "fastapi", "pandas", "script", "notebook",
            "data",
        ],
    ),
    (
        Stack::Node,
        &[
            "typescript",
            "javascript",
            "node",
            "npm",
            "react",
            "svelte",
            "vue",
            "web",
            "website",
            "frontend",
            "browser",
            "extension",
        ],
    ),
    (
        Stack::Go,
        &[
            "go",
            "golang",
            "server",
            "service",
            "api",
            "microservice",
            "daemon",
        ],
    ),
];

/// The stack an idea suggests among the `installed` ones, with its reason.
/// A named language wins over a kind of software (`CLI`, `web`, `API`);
/// a stack whose tool is not installed is never suggested; with none
/// installed: [`Stack::None`].
#[must_use]
pub fn suggest_stack(idea: &str, installed: &[(Stack, String)]) -> StackPick {
    let lower = ascii_fold(idea).to_ascii_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter(|w| !w.is_empty())
        .collect();
    let version = |s: Stack| {
        installed
            .iter()
            .find(|(k, _)| *k == s)
            .map(|(_, v)| v.as_str())
    };
    let tool_line = |s: Stack| {
        let tool = s.tool().unwrap_or("");
        match version(s) {
            Some(v) if !v.is_empty() => format!("{tool} {v} is installed"),
            _ => format!("{tool} is installed"),
        }
    };
    // A language named outright first (the first three words of each cue list).
    for pass in [true, false] {
        for (stack, cues) in CUES {
            if version(*stack).is_none() {
                continue;
            }
            let pool: &[&str] = if pass {
                &cues[..cues.len().min(3)]
            } else {
                cues
            };
            if let Some(w) = words.iter().find(|w| pool.contains(w)) {
                let shown = if w.len() <= 4 {
                    w.to_ascii_uppercase()
                } else {
                    (*w).to_owned()
                };
                return StackPick {
                    id: *stack,
                    detected: false,
                    reason: format!("\"{shown}\" and {}", tool_line(*stack)),
                };
            }
        }
    }
    match Stack::ALL.into_iter().find(|s| version(*s).is_some()) {
        Some(s) => StackPick {
            id: s,
            detected: false,
            reason: format!("{} — nothing in the idea names a stack", tool_line(s)),
        },
        None => StackPick {
            id: Stack::None,
            detected: false,
            reason: "no stack tool is installed: add code yourself".to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slug_takes_five_significant_words_folded() {
        assert_eq!(
            slug("a CLI that turns Markdown tables into CSV"),
            "cli-turns-markdown-tables-csv"
        );
        assert_eq!(slug("Ünïcödé café ☕"), "unicode-cafe");
        assert_eq!(slug("   "), "project");
        assert_eq!(slug("the a of"), "project");
        let long = slug("supercalifragilisticexpialidocious extraordinarily verbose naming");
        assert!(long.len() <= SLUG_MAX, "{long}");
        assert_eq!(slug("md2csv"), "md2csv");
    }

    #[test]
    fn a_named_language_wins_and_missing_tools_are_never_suggested() {
        let all = [
            (Stack::Rust, "1.91.0".to_owned()),
            (Stack::Python, "0.9.0".to_owned()),
            (Stack::Node, "11.0.0".to_owned()),
            (Stack::Go, "1.25".to_owned()),
        ];
        let s = suggest_stack("a CLI that turns Markdown tables into CSV", &all);
        assert_eq!(s.id, Stack::Rust);
        assert_eq!(s.reason, "\"CLI\" and cargo 1.91.0 is installed");
        assert_eq!(
            suggest_stack("a python script for my photos", &all).id,
            Stack::Python
        );
        assert_eq!(suggest_stack("a web dashboard", &all).id, Stack::Node);
        assert_eq!(suggest_stack("an API in go", &all).id, Stack::Go);
        let no_python = [(Stack::Rust, "1.91.0".to_owned())];
        assert_eq!(suggest_stack("a python script", &no_python).id, Stack::Rust);
        assert_eq!(suggest_stack("anything", &[]).id, Stack::None);
    }

    #[test]
    fn extensions_are_classified_and_urls_name_their_host() {
        assert_eq!(ExtensionChoice::of("bug").kind, ExtensionKind::Bundled);
        assert_eq!(
            ExtensionChoice::of("~/x/threatspec").kind,
            ExtensionKind::Path
        );
        let u = ExtensionChoice::of("https://user@example.com:8443/x.zip");
        assert_eq!(u.kind, ExtensionKind::Url);
        assert_eq!(u.host().as_deref(), Some("example.com"));
    }

    #[test]
    fn a_plan_lists_its_network_hosts_once() {
        let step = |id: &str, access, hosts: &[&str]| PlanStep {
            id: id.to_owned(),
            argv: vec!["x".to_owned()],
            cwd: "/t".to_owned(),
            access,
            hosts: hosts.iter().map(|h| (*h).to_owned()).collect(),
            writes: String::new(),
            limit_s: LOCAL_LIMIT_S,
            note: None,
        };
        let plan = ProjectPlan {
            target: "/t".to_owned(),
            name: "t".to_owned(),
            mode: ProjectMode::NewFolder,
            stack: StackPick {
                id: Stack::Python,
                detected: false,
                reason: String::new(),
            },
            stacks: Vec::new(),
            format: "speckit".to_owned(),
            formats: Vec::new(),
            extensions: Vec::new(),
            preset: true,
            steps: vec![
                step("a", StepAccess::Local, &[]),
                step("b", StepAccess::Network, &["pypi.org"]),
                step(
                    "c",
                    StepAccess::Network,
                    &["pypi.org", "files.pythonhosted.org"],
                ),
            ],
            sandbox: SandboxNote { backend: None },
            identity: None,
            refusal: None,
            observed: ObservedFolder::default(),
            digest: String::new(),
        };
        assert_eq!(
            plan.network_hosts(),
            vec!["pypi.org", "files.pythonhosted.org"]
        );
    }
}
