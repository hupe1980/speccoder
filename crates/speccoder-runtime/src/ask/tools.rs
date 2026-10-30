//! Ask's readers beyond the workspace tools: history, the journal's
//! account, evidence and the catalog of codes. Each reads through a
//! runtime function or a fixed git argv — never a shell, never the network.

use std::path::{Path, PathBuf};

use agentplane::model::ToolDeclaration;
use agentplane::tools::{Destination, ToolClient, ToolError, ToolId};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::git::Git;
use crate::tools::paths::normalise;
use crate::tools::policy::Policy;

/// The tool server's name.
pub const SERVER: &str = "ask";

/// The workspace tools Ask may use: reading only.
pub const WORKSPACE: [&str; 5] = crate::substrate::converge::READ_ONLY;

/// Commits on a path, newest first.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GitLog {
    /// A file or directory; the whole repository when absent.
    pub path: Option<String>,
    /// How many commits (at most 50; 20 when absent).
    pub n: Option<u32>,
}

/// A file as it was at a revision, or a commit's summary.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GitShow {
    /// A commit, tag or `HEAD~2`-style revision.
    pub rev: String,
    /// The file at that revision; the commit's summary when absent.
    pub path: Option<String>,
}

/// What the model said in a run, turn by turn (its words: a claim).
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JournalAccount {
    /// The run id.
    pub run: String,
    /// The first turn (1-based).
    pub from: Option<u32>,
    /// The last turn.
    pub to: Option<u32>,
}

/// A task's state and its evidence rows.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskEvidence {
    /// The task id (`T004`).
    pub task: String,
    /// The feature; the current one when absent.
    pub feature: Option<String>,
}

/// The tasks of a feature and their states.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Report {
    /// The feature; the current one when absent.
    pub feature: Option<String>,
}

/// What waits for a person, across features.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Inbox {}

/// What a speccoder code means and how to resolve it.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Explain {
    /// The code (`BUDGET-EXHAUSTED`).
    pub code: String,
}

/// A change for the person to confirm; nothing runs.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Propose {
    /// `quick_change`, `bug_fix`, `nudge`, `answer` or `start_spec`.
    pub kind: String,
    /// One line, at most 120 characters.
    pub title: String,
    /// The task (nudge) or open question (answer) it is about.
    pub target: Option<String>,
    /// A quick change, in words the implementer can act on.
    pub request: Option<String>,
    /// When a quick change is done (at most 8).
    pub checks: Option<Vec<String>>,
    /// A nudge's note for the task's next attempt.
    pub note: Option<String>,
    /// An open question's answer.
    pub answer: Option<String>,
    /// A new spec's description.
    pub description: Option<String>,
    /// A bug's current behaviour.
    pub symptom: Option<String>,
    /// What should happen instead (at least one for a bug).
    pub expected: Option<Vec<String>>,
    /// What must stay as it is.
    pub unchanged: Option<Vec<String>>,
}

fn declaration<T: JsonSchema>(name: &str, description: &str) -> ToolDeclaration {
    ToolDeclaration::new(name, description, agentplane::tools::strict_schema::<T>())
}

/// Ask's own tools, as the model is told them.
#[must_use]
pub fn declarations() -> Vec<ToolDeclaration> {
    vec![
        declaration::<GitLog>(
            "git_log",
            "Commits on a path, newest first: hash, date, author, subject.",
        ),
        declaration::<GitShow>(
            "git_show",
            "A file as it was at a revision, or a commit's summary and changed files.",
        ),
        declaration::<JournalAccount>(
            "journal_account",
            "What the implementing model said in a run, turn by turn — its words, a claim, never evidence.",
        ),
        declaration::<TaskEvidence>(
            "task_evidence",
            "A task's state and the evidence speccoder recorded for it.",
        ),
        declaration::<Report>("report", "A feature's tasks and their states."),
        declaration::<Inbox>(
            "inbox",
            "What waits for a person's decision, across features.",
        ),
        declaration::<Explain>(
            "explain",
            "What a speccoder code (e.g. BUDGET-EXHAUSTED) means and how to resolve it.",
        ),
        declaration::<Propose>(
            crate::ask::PROPOSE,
            "Suggest a change for the person to review in a sheet: a quick change, a bug fix, a nudge for a task, an answer to an open question, or a new spec. Nothing runs; only the person can confirm. At most one per turn.",
        ),
    ]
}

/// The readers' names, for the manifest's grants.
#[must_use]
pub fn names() -> Vec<String> {
    declarations()
        .into_iter()
        .map(|d| d.name)
        .filter(|n| n != crate::ask::PROPOSE)
        .collect()
}

/// Ask's readers over one project.
#[derive(Debug, Clone)]
pub struct AskTools {
    root: PathBuf,
    /// The project's journal (its runs' accounts).
    journal: PathBuf,
    policy: Policy,
}

impl AskTools {
    /// Readers over the project at `root`, whose runs are journaled in `journal`.
    #[must_use]
    pub fn new(root: &Path, journal: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            journal: journal.to_path_buf(),
            policy: Policy::new(&[], &[]),
        }
    }

    fn path(&self, p: &str) -> Result<String, String> {
        let p = normalise(p).map_err(|e| e.0)?;
        if self.policy.protected().is_secret(&p) {
            return Err(format!("`{p}` looks like a credential"));
        }
        Ok(p)
    }

    async fn run(&self, tool: &str, args: &Value) -> Value {
        let parsed = |v: &Value| v.clone();
        match tool {
            "git_log" => match serde_json::from_value::<GitLog>(parsed(args)) {
                Ok(a) => self.git_log(&a),
                Err(e) => fail(e),
            },
            "git_show" => match serde_json::from_value::<GitShow>(parsed(args)) {
                Ok(a) => self.git_show(&a),
                Err(e) => fail(e),
            },
            "journal_account" => match serde_json::from_value::<JournalAccount>(parsed(args)) {
                Ok(a) => self.journal_account(&a).await,
                Err(e) => fail(e),
            },
            // The status readers open runtimes of their own: off this one.
            "task_evidence" => match serde_json::from_value::<TaskEvidence>(parsed(args)) {
                Ok(a) => off_runtime(|| self.task_evidence(&a)),
                Err(e) => fail(e),
            },
            "report" => match serde_json::from_value::<Report>(parsed(args)) {
                Ok(a) => off_runtime(|| self.report(a.feature.as_deref())),
                Err(e) => fail(e),
            },
            "inbox" => off_runtime(|| self.inbox()),
            "explain" => match serde_json::from_value::<Explain>(parsed(args)) {
                Ok(a) => explain(&a.code),
                Err(e) => fail(e),
            },
            other => fail(format!("no Ask tool `{other}`")),
        }
    }

    fn git_log(&self, a: &GitLog) -> Value {
        let n = a.n.unwrap_or(20).clamp(1, 50).to_string();
        let mut args = vec![
            "-c",
            "core.pager=cat",
            "log",
            "--no-color",
            "--date=short",
            "--format=%h%x1f%ad%x1f%an%x1f%s",
            "-n",
            &n,
        ];
        let path;
        if let Some(p) = &a.path {
            path = match self.path(p) {
                Ok(p) => p,
                Err(e) => return fail(e),
            };
            args.extend(["--", path.as_str()]);
        }
        match Git::at(&self.root).run(&args) {
            Ok(out) => {
                let commits: Vec<Value> = String::from_utf8_lossy(&out)
                    .lines()
                    .filter_map(|l| {
                        let f: Vec<&str> = l.split('\u{1f}').collect();
                        (f.len() == 4).then(|| json!({ "commit": f[0], "date": f[1], "author": f[2], "subject": f[3] }))
                    })
                    .collect();
                json!({ "ok": true, "commits": commits })
            }
            Err(e) => fail(e.0),
        }
    }

    fn git_show(&self, a: &GitShow) -> Value {
        // A revision, never an option: letters, digits and `._/~^-`, not first `-`.
        let rev = a.rev.trim();
        if rev.is_empty()
            || rev.starts_with('-')
            || !rev
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._/~^-".contains(c))
        {
            return fail(format!("`{rev}` is not a revision"));
        }
        let out = match &a.path {
            Some(p) => match self.path(p) {
                Ok(p) => Git::at(&self.root).run(&[
                    "-c",
                    "core.pager=cat",
                    "show",
                    "--no-color",
                    &format!("{rev}:{p}"),
                ]),
                Err(e) => return fail(e),
            },
            None => Git::at(&self.root).run(&[
                "-c",
                "core.pager=cat",
                "show",
                "--no-color",
                "--stat",
                "--format=%H%n%ad%n%an%n%n%B",
                "--date=short",
                rev,
            ]),
        };
        match out {
            Ok(out) => {
                let mut text = crate::secrets::mask(&String::from_utf8_lossy(&out));
                let truncated = text.len() > crate::tools::READ_CAP;
                if truncated {
                    let mut cut = crate::tools::READ_CAP;
                    while !text.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    text.truncate(cut);
                }
                json!({ "ok": true, "content": text, "truncated": truncated })
            }
            Err(e) => fail(e.0),
        }
    }

    async fn journal_account(&self, a: &JournalAccount) -> Value {
        match crate::substrate::account(&self.journal, &a.run).await {
            Ok(turns) => {
                let from = a.from.unwrap_or(1).max(1) as usize;
                let to = a.to.map_or(usize::MAX, |t| t as usize);
                let turns: Vec<Value> = turns
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| (from..=to).contains(&(i + 1)))
                    .map(|(i, t)| json!({ "turn": i + 1, "account": t }))
                    .collect();
                json!({ "ok": true, "run": a.run, "note": "the implementing model's words: a claim, never evidence", "turns": turns })
            }
            Err(e) => fail(e.0),
        }
    }

    fn task_evidence(&self, a: &TaskEvidence) -> Value {
        let report = crate::status::report(&self.root, a.feature.as_deref());
        let Some(speccoder_core::CommandData::Report(r)) = report.data() else {
            return fail(report.message().to_owned());
        };
        let Some(t) = r.tasks.iter().find(|t| t.id == a.task) else {
            return fail(format!("no task {} in {}", a.task, r.feature));
        };
        let statements = t
            .commit
            .as_deref()
            .map(|c| crate::evidence::statements(&self.root, c))
            .unwrap_or_default();
        json!({
            "ok": true,
            "feature": r.feature,
            "task": { "id": t.id, "title": t.title, "state": t.state, "commit": t.commit },
            "rows": crate::evidence::rows(t, &statements, None),
        })
    }

    fn report(&self, feature: Option<&str>) -> Value {
        let report = crate::status::report(&self.root, feature);
        let Some(speccoder_core::CommandData::Report(r)) = report.data() else {
            return fail(report.message().to_owned());
        };
        let tasks: Vec<Value> = r
            .tasks
            .iter()
            .map(|t| json!({ "id": t.id, "title": t.title, "state": t.state, "phase": t.phase }))
            .collect();
        json!({ "ok": true, "feature": r.feature, "tasks": tasks })
    }

    fn inbox(&self) -> Value {
        let inbox = crate::inbox::inbox(&self.root, None);
        match inbox.data() {
            Some(speccoder_core::CommandData::Inbox(i)) => {
                json!({ "ok": true, "inbox": serde_json::to_value(i).unwrap_or(Value::Null) })
            }
            _ => fail(inbox.message().to_owned()),
        }
    }
}

fn explain(code: &str) -> Value {
    match speccoder_core::codes::lookup(code.trim()) {
        Some(e) => {
            json!({ "ok": true, "code": e.name, "title": e.title, "explain": e.explain, "fix": e.fix, "docs": e.docs() })
        }
        None => fail(format!("no code `{code}`")),
    }
}

/// `f` on a thread of its own: a reader that blocks on a runtime of its
/// own cannot run inside the plane's.
fn off_runtime(f: impl FnOnce() -> Value + Send) -> Value {
    std::thread::scope(|s| {
        s.spawn(f)
            .join()
            .unwrap_or_else(|_| fail("the reader failed"))
    })
}

fn fail(error: impl ToString) -> Value {
    json!({ "ok": false, "error": error.to_string() })
}

#[async_trait]
impl ToolClient for AskTools {
    async fn call(
        &self,
        tool: &ToolId,
        arguments: &Value,
        _provenance: Option<&agentplane::core::Provenance>,
    ) -> Result<Value, ToolError> {
        if tool.server != SERVER || !names().contains(&tool.tool) {
            return Err(ToolError::Refused {
                tool: tool.clone(),
                detail: "not an Ask tool".to_owned(),
            });
        }
        Ok(self.run(&tool.tool, arguments).await)
    }

    fn destination(&self, _tool: &ToolId) -> Destination {
        Destination::Local
    }
}
