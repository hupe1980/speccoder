//! A checkout's agent files: what a checkout tells agents —
//! instructions, skills, an executor's settings — and whether a person
//! trusted each one as it is now. Agents read only trusted bytes.

use serde::{Deserialize, Serialize};

/// What an agent file is.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentFileKind {
    /// Text that steers a model (`AGENTS.md`, `CLAUDE.md`, rules).
    Instructions,
    /// A skill a model may run.
    Skill,
    /// An executor's settings: hooks and servers it starts itself.
    Settings,
}

impl AgentFileKind {
    /// A word for it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Instructions => "instructions",
            Self::Skill => "skill",
            Self::Settings => "settings",
        }
    }
}

/// Who reads an agent file.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Reader {
    /// speccoder's own model.
    Speccoder,
    /// Claude Code.
    Claude,
    /// Codex.
    Codex,
    /// Gemini CLI.
    Gemini,
    /// OpenCode.
    Opencode,
    /// GitHub Copilot.
    Copilot,
}

impl Reader {
    /// Every reader.
    pub const ALL: [Self; 6] = [
        Self::Speccoder,
        Self::Claude,
        Self::Codex,
        Self::Gemini,
        Self::Opencode,
        Self::Copilot,
    ];

    /// Its name, as executors are named in settings.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Speccoder => "speccoder",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Opencode => "opencode",
            Self::Copilot => "copilot",
        }
    }

    /// The reader an executor's agent name is, if known.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == name)
    }
}

/// Where an agent file stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentFileState {
    /// Trusted as it is now.
    Trusted,
    /// Changed since it was trusted.
    Changed,
    /// Never trusted.
    Untrusted,
    /// Trusted once, gone now.
    Gone,
}

impl AgentFileState {
    /// Whether it waits for a person.
    #[must_use]
    pub const fn waits(self) -> bool {
        !matches!(self, Self::Trusted)
    }

    /// A word for it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Trusted => "trusted",
            Self::Changed => "changed",
            Self::Untrusted => "new",
            Self::Gone => "gone",
        }
    }
}

/// Who trusted a file, how and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TrustedBy {
    /// The person.
    pub by: String,
    /// Through what: `terminal`, `web`, `speccoder` (files it wrote).
    pub via: String,
    /// When (RFC 3339).
    pub at: String,
}

/// One agent file of a checkout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentFile {
    /// Repository-relative, `/`-separated.
    pub path: String,
    /// What it is.
    pub kind: AgentFileKind,
    /// Who reads it.
    pub readers: Vec<Reader>,
    /// `sha256:…` of its bytes now; empty when gone.
    pub digest: String,
    /// Its size now.
    pub bytes: u64,
    /// Where it stands.
    pub state: AgentFileState,
    /// The record in force, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted: Option<TrustedBy>,
    /// Where these bytes are when not in your checkout: a feature's
    /// worktree (`001-login`), whose agents read them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<String>,
    /// What changed (a unified diff), or the text of a new file; cut at
    /// [`DIFF_CAP`] with a note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
}

/// The longest diff or text shown for one file.
pub const DIFF_CAP: usize = 64 * 1024;

/// `trust`: a checkout's agent files.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TrustReport {
    /// Every agent file found or trusted before, waiting ones first.
    pub files: Vec<AgentFile>,
    /// How many wait for a person.
    pub waiting: usize,
}

impl TrustReport {
    /// A report of `files`, waiting ones first, then by path.
    #[must_use]
    pub fn new(mut files: Vec<AgentFile>) -> Self {
        files.sort_by(|a, b| {
            b.state
                .waits()
                .cmp(&a.state.waits())
                .then_with(|| a.path.cmp(&b.path))
        });
        let waiting = files.iter().filter(|f| f.state.waits()).count();
        Self { files, waiting }
    }

    /// The Needs You title for what waits, if anything does.
    #[must_use]
    pub fn title(&self) -> Option<String> {
        let waiting: Vec<&AgentFile> = self.files.iter().filter(|f| f.state.waits()).collect();
        let changed = waiting
            .iter()
            .all(|f| matches!(f.state, AgentFileState::Changed | AgentFileState::Gone));
        match (waiting.as_slice(), changed) {
            ([], _) => None,
            ([one], true) => Some(format!("{} changed since you trusted it", one.path)),
            ([one], false) => Some(format!("{} waits for your trust", one.path)),
            (many, true) => Some(format!(
                "{} agent files changed since you trusted them",
                many.len()
            )),
            (many, false) => Some(format!("{} agent files wait for your trust", many.len())),
        }
    }
}

/// `text` cut at [`DIFF_CAP`] on a character boundary, with a note.
#[must_use]
pub fn capped(text: &str) -> String {
    if text.len() <= DIFF_CAP {
        return text.to_owned();
    }
    let mut end = DIFF_CAP;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n[… cut: {} KiB shown of {} KiB]\n",
        &text[..end],
        DIFF_CAP / 1024,
        text.len().div_ceil(1024)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, state: AgentFileState) -> AgentFile {
        AgentFile {
            path: path.to_owned(),
            kind: AgentFileKind::Instructions,
            readers: vec![Reader::Speccoder],
            digest: String::new(),
            bytes: 0,
            state,
            trusted: None,
            tree: None,
            diff: None,
        }
    }

    #[test]
    fn waiting_files_come_first_and_name_the_title() {
        let r = TrustReport::new(vec![
            file("b.md", AgentFileState::Trusted),
            file("AGENTS.md", AgentFileState::Changed),
        ]);
        assert_eq!(r.waiting, 1);
        assert_eq!(r.files[0].path, "AGENTS.md");
        assert_eq!(
            r.title().as_deref(),
            Some("AGENTS.md changed since you trusted it")
        );
        let r = TrustReport::new(vec![
            file("AGENTS.md", AgentFileState::Changed),
            file(".claude/settings.json", AgentFileState::Untrusted),
        ]);
        assert_eq!(
            r.title().as_deref(),
            Some("2 agent files wait for your trust")
        );
        assert_eq!(
            TrustReport::new(vec![file("a", AgentFileState::Trusted)]).title(),
            None
        );
    }

    #[test]
    fn long_text_is_cut_with_a_note() {
        let t = "é".repeat(DIFF_CAP);
        let c = capped(&t);
        assert!(c.len() < t.len() + 100);
        assert!(c.contains("[… cut"));
        assert_eq!(capped("short"), "short");
    }

    #[test]
    fn readers_round_trip_by_name() {
        for r in Reader::ALL {
            assert_eq!(Reader::named(r.as_str()), Some(r));
        }
        assert_eq!(Reader::named("cursor"), None);
    }
}
