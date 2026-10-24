//! Where content came from.
//!
//! agentplane's gates — what may go to which model, which arguments may
//! drive a mutating tool — decide on labels. So every value speccoder hands
//! the runtime says where it came from: speccoder's own instructions and the
//! person's committed spec and decisions are trusted; a check's output, the
//! worktree's content, an MCP server's words and the model's answers are not.
//!
//! Labels are given where content enters — a skill input is assembled from
//! labelled parts — and read back with `label_at`, never inferred from what
//! the content looks like: agentplane journals the field labels a run was
//! admitted with (0.50), live and in replay.

use agentplane::core::{Label, SourceId, Tainted};
use serde_json::Value;

/// Check output quoted in a retry digest: the verifier's facts.
pub const VERIFIER: &str = "speccoder://verifier";

/// Worktree content speccoder inlines into a prompt (a patch, files, a manifest).
pub const WORKTREE: &str = "speccoder://worktree";

/// A person's answer to a question (F-072): their input, not speccoder's;
/// who they were is on the decision's record, not in the label.
pub const PERSON: &str = "speccoder://person";

/// The source of a tool's results and of an MCP tool's description.
#[must_use]
pub fn tool(server: &str, name: &str) -> SourceId {
    agentplane::tools::ToolId::new(server, name).source()
}

/// An untrusted label from `source`.
#[must_use]
pub fn from(source: &str) -> Label {
    Label::untrusted(SourceId::new(source))
}

/// A prompt whose `/system` is speccoder's constant instruction (trusted)
/// and whose every other part carries `label`.
#[must_use]
pub fn labelled_prompt(prompt: Value, label: &Label) -> Tainted<Value> {
    let Value::Object(parts) = prompt else {
        return Tainted::with_label(prompt, label.clone());
    };
    Tainted::object(parts.into_iter().map(|(k, v)| {
        let part = if k == "system" {
            Tainted::trusted(v)
        } else {
            Tainted::with_label(v, label.clone())
        };
        (k, part)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_instruction_stays_trusted_and_the_rest_carries_its_sources() {
        let p = labelled_prompt(
            json!({ "system": "be careful", "messages": [{ "role": "user", "content": "x" }] }),
            &from(VERIFIER),
        );
        assert!(!p.label_at("/system").is_some_and(Label::is_untrusted));
        assert!(p.label_at("/messages").is_some_and(Label::is_untrusted));
        assert!(p.label().is_untrusted());
    }
}
