//! What one message said about a tool call, and the call those messages
//! add up to.
//!
//! The protocol lets every report after the first carry only what changed,
//! and a permission request may name a call by its id alone. So a report is
//! kept as sent — nothing filled in — and one record per call id is kept in
//! which a field present replaces the stored value and a field absent or
//! `null` leaves it alone. The policy reads that record.
//!
//! Adapted from devplane's `acp::calls` (same owner, MIT OR Apache-2.0).

use std::path::PathBuf;

/// One message's account of a tool call. Every field but the id is `None`
/// when the message did not carry it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolReport {
    /// The call's id.
    pub id: String,
    /// For a person.
    pub title: Option<String>,
    /// The protocol's spelling: `execute`, `edit`, …
    pub kind: Option<String>,
    /// `pending`, `in_progress`, `completed`, `failed`.
    pub status: Option<String>,
    /// The arguments the agent passed.
    pub raw_input: Option<serde_json::Value>,
    /// `Some(vec![])` is *the agent said none*; `None` is *not said*.
    pub locations: Option<Vec<PathBuf>>,
}

impl ToolReport {
    /// From the protocol's own JSON shape of a tool call or an update: an
    /// absent key and a `null` are the same `None`.
    #[must_use]
    pub fn from_wire(id: String, fields: &serde_json::Value) -> Self {
        let text = |k: &str| fields.get(k).and_then(|v| v.as_str()).map(str::to_owned);
        Self {
            id,
            title: text("title"),
            kind: text("kind"),
            status: text("status"),
            raw_input: fields.get("rawInput").filter(|v| !v.is_null()).cloned(),
            locations: fields
                .get("locations")
                .and_then(|v| v.as_array())
                .map(|ls| {
                    ls.iter()
                        .filter_map(|l| l.get("path").and_then(|p| p.as_str()))
                        .map(PathBuf::from)
                        .collect()
                }),
        }
    }
}

/// What an agent is asking to do, in the protocol's terms: the title is
/// prose for a person; the kind, the raw input and the locations are what
/// the policy reads — never the title.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolRequest {
    /// The call's id.
    pub id: String,
    /// For a person; `None` until the agent sends one.
    pub title: Option<String>,
    /// `read`, `edit`, `delete`, `move`, `search`, `execute`, `think`,
    /// `fetch`, `switch_mode`, `other`.
    pub kind: Option<String>,
    /// The arguments the agent passed.
    pub raw_input: Option<serde_json::Value>,
    /// Paths the call names.
    pub locations: Vec<PathBuf>,
    /// The latest status reported.
    pub status: Option<String>,
}

impl ToolRequest {
    /// Folds a later report in: a field it carries replaces the stored one;
    /// a field it omits leaves it alone.
    pub fn merge(&mut self, r: &ToolReport) {
        if let Some(t) = &r.title {
            self.title = Some(t.chars().take(200).collect());
        }
        if let Some(k) = &r.kind {
            self.kind = Some(k.clone());
        }
        if let Some(i) = &r.raw_input {
            self.raw_input = Some(i.clone());
        }
        if let Some(l) = &r.locations {
            self.locations.clone_from(l);
        }
        if let Some(s) = &r.status {
            self.status = Some(s.clone());
        }
    }

    /// The title for a person: the agent's, or `untitled call <id>`.
    #[must_use]
    pub fn shown_title(&self) -> String {
        match self.title.as_deref() {
            Some(t) if !t.trim().is_empty() => t.to_owned(),
            _ => format!("untitled call {}", self.id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn wire(v: serde_json::Value) -> ToolReport {
        ToolReport::from_wire("c1".into(), &v)
    }

    #[test]
    fn a_present_field_replaces_and_an_absent_or_null_one_keeps() {
        let mut call = ToolRequest {
            id: "c1".into(),
            ..ToolRequest::default()
        };
        call.merge(&wire(json!({
            "title": "Run a command", "kind": "execute",
            "rawInput": {"command": "ls"}, "locations": [{"path": "/a"}]
        })));
        call.merge(&wire(json!({"rawInput": {"command": "rm -rf build"}})));
        assert_eq!(call.title.as_deref(), Some("Run a command"));
        assert_eq!(call.raw_input, Some(json!({"command": "rm -rf build"})));
        call.merge(&wire(
            json!({"title": null, "kind": null, "rawInput": null, "locations": null}),
        ));
        assert_eq!(call.kind.as_deref(), Some("execute"), "null keeps");
        assert_eq!(call.locations, vec![PathBuf::from("/a")]);
        call.merge(&wire(json!({"locations": []})));
        assert!(call.locations.is_empty(), "an empty list is said");
        assert_eq!(
            ToolRequest {
                id: "t9".into(),
                ..ToolRequest::default()
            }
            .shown_title(),
            "untitled call t9"
        );
    }
}
