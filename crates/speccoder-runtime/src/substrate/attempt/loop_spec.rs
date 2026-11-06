//! What a governed tool loop is given, what it has left, and what it shows
//! as it goes.

use super::*;

/// What a governed tool loop offers the model and asks of it.
pub(crate) struct LoopSpec {
    /// The tools declared to the model; any other name is refused.
    pub tools: Vec<ToolDeclaration>,
    /// Classifies every call before dispatch.
    pub policy: Policy,
    /// Model turns.
    pub max_turns: u32,
    /// Tool calls.
    pub max_tool_calls: u32,
    /// Ask each turn for the model's reasoning summary.
    pub reasoning: bool,
    /// The closing answer's schema; `None`: a text answer ends the turn.
    pub schema: Option<Value>,
    /// Declared tools served elsewhere: (name, server, tool) — MCP.
    pub routes: Vec<(String, String, String)>,
    /// The workspace's absolute path, from the journaled input: absolute
    /// paths inside it are made relative before the policy sees them.
    pub root: Option<String>,
    /// A write to a frozen test that only adds lines needs no approval.
    pub frozen_additions: bool,
    /// The status for a closing answer.
    pub finish: fn(&Value) -> &'static str,
    /// Ask a person instead of stopping when a call needs approval.
    pub ask: Option<Asker>,
    /// Objects a person approved for this run: allowed without asking.
    pub run_approved: Vec<Object>,
    /// Objects a person approved for one call each: the first call of
    /// each is allowed without asking, later ones ask again.
    pub once: Vec<Object>,
    /// Live progress: the task ID and where to report.
    pub observer: Option<(String, std::sync::Arc<dyn crate::progress::Progress>)>,
    /// The run the attempt belongs to (`run-<secs>-<hex>`).
    pub run: Option<String>,
    /// How a run asks: its count so far, its budget and whether a person is
    /// prompted at all.
    pub asking: Asking,
    /// Untrusted content the prompt carries (inlined worktree text, the
    /// verifier's digest): what derived arguments are found in.
    pub parts: Vec<crate::approvals::derived::Part>,
    /// MCP tools' digests by model-facing name: their objects.
    pub mcp_digests: std::collections::BTreeMap<String, String>,
    /// What a `report_blocked` call's evidence is checked against.
    pub stop: StopGate,
    /// Tokens a turn may use (Ask); `None`: the plane's budget alone.
    pub max_tokens: Option<u64>,
    /// What is left of the run's cap for this attempt.
    pub left: Option<Left>,
    /// The first read id to issue (`r<n>`), when the answer cites reads.
    pub reads: Option<u32>,
    /// Pressed when a person stops the turn (Ask): the model call streaming
    /// ends where it is (a recorded, metered completion) and none of its
    /// calls run. Not effect identity — replay returns the recorded call.
    pub halt: Option<agentplane::model::Stop>,
}

/// A run's asking budget as an attempt sees it (all journaled).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asking {
    /// Prompts shown to a person in the run so far.
    pub asked: u32,
    /// `[approvals] budget`: at most this many per run.
    pub budget: u32,
    /// A person is prompted inline; `false` with `--no-prompt` (calls wait
    /// as ordinary approvals, nothing is counted).
    pub prompts: bool,
}

impl Default for Asking {
    fn default() -> Self {
        Self {
            asked: 0,
            budget: DEFAULT_BUDGET,
            prompts: false,
        }
    }
}

/// `[approvals] budget` unless configured.
pub use crate::substrate::run_config::DEFAULT_BUDGET;

/// What is left of the run's cap for one attempt, and the prices a dollar
/// cap is counted at (journaled with the attempt's settings).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Left {
    /// Tokens or dollars left.
    pub cap: crate::substrate::run_config::Cap,
    /// `[model]`'s prices; a dollar cap is refused without them.
    pub prices: Option<crate::substrate::model_config::Prices>,
}

impl Left {
    /// What `usage` spent, in the cap's own unit (tokens, or millionths of a dollar).
    #[must_use]
    pub fn spent(&self, usage: &agentplane::model::Usage) -> u64 {
        match self.cap {
            crate::substrate::run_config::Cap::Tokens(_) => {
                usage.input_tokens + usage.output_tokens
            }
            crate::substrate::run_config::Cap::Usd(_) => self.prices.map_or(0, |p| {
                micros(p.usd(
                    usage.input_tokens,
                    usage.output_tokens,
                    usage.cache_read_tokens,
                    usage.cache_write_tokens,
                ))
            }),
        }
    }

    /// Whether `usage` used all that is left.
    #[must_use]
    pub fn reached(&self, usage: &agentplane::model::Usage) -> bool {
        self.spent(usage) >= self.cap.amount()
    }

    /// The stop's words: what the attempt used of what was left.
    #[must_use]
    pub fn reason(&self, usage: &agentplane::model::Usage) -> String {
        format!(
            "the run's cap: this attempt used {} of the {} left (`run --cap`, `[budget] cap`)",
            self.cap.with(self.spent(usage)).rounded(),
            self.cap.rounded()
        )
    }
}

/// A recorded usage object (`input_tokens`, …) as the model's usage.
#[must_use]
pub fn usage_of(v: Option<&Value>) -> agentplane::model::Usage {
    let n = |k: &str| v.and_then(|v| v[k].as_u64()).unwrap_or(0);
    agentplane::model::Usage {
        input_tokens: n("input_tokens"),
        output_tokens: n("output_tokens"),
        cache_read_tokens: n("cache_read_tokens"),
        cache_write_tokens: n("cache_write_tokens"),
        ..agentplane::model::Usage::default()
    }
}

/// Dollars as millionths, rounded.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn micros(usd: f64) -> u64 {
    (usd * 1e6).round().max(0.0) as u64
}

/// Turns an absolute `path` argument inside the workspace `root` into a
/// workspace-relative one. Tools print absolute paths (Spec Kit's scripts
/// do), and a model passes them on; outside the root a path stays as it is
/// — and the policy refuses it.
pub(super) fn relativise(arguments: &mut Value, root: Option<&str>) {
    let (Some(root), Some(path)) = (root, arguments.get("path").and_then(Value::as_str)) else {
        return;
    };
    let root = root.trim_end_matches('/');
    if let Some(rel) = path.strip_prefix(root).and_then(|r| r.strip_prefix('/')) {
        let rel = if rel.is_empty() {
            ".".to_owned()
        } else {
            rel.to_owned()
        };
        arguments["path"] = Value::String(rel);
    }
}

/// Reports one logged tool call to whoever watches.
pub(super) fn emit(
    observer: Option<&(String, std::sync::Arc<dyn crate::progress::Progress>)>,
    turn: u32,
    entry: &Value,
) {
    emit_with_tail(observer, turn, entry, Vec::new());
}

/// [`emit`] with the last lines of a failed command's output (live only,
/// never journaled).
pub(super) fn emit_with_tail(
    observer: Option<&(String, std::sync::Arc<dyn crate::progress::Progress>)>,
    turn: u32,
    entry: &Value,
    tail: Vec<String>,
) {
    if let Some((task, p)) = observer {
        let tool = entry["tool"].as_str().unwrap_or_default();
        p.event(&crate::progress::Event::ToolCall {
            task: task.clone(),
            turn,
            tool: tool.to_owned(),
            target: crate::progress::target(tool, &entry["arguments"]),
            decision: entry["decision"].as_str().unwrap_or_default().to_owned(),
            ok: entry["ok"].as_bool(),
            tail,
            contained: entry["contained"] == true,
        });
    }
}
