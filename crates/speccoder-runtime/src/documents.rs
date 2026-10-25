//! A feature's planning documents, rendered for an interface (F-076): the
//! spec, the plan and the task list as HTML. Raw HTML in them is shown as
//! text, never passed through: the documents are the repository's, and an
//! interface that holds a token must not run what they contain.

use std::path::Path;

use pulldown_cmark::{Event, Options, Parser, html};
use serde::Serialize;

/// One rendered document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Document {
    /// `spec`, `plan`, `tasks`, …
    pub name: String,
    /// Repository-relative path.
    pub path: String,
    /// The rendered HTML.
    pub html: String,
}

/// Renders Markdown; raw HTML becomes escaped text.
#[must_use]
pub fn render(markdown: &str) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_HEADING_ATTRIBUTES;
    let events = Parser::new_ext(markdown, options).map(|e| match e {
        Event::Html(t) | Event::InlineHtml(t) => Event::Text(t),
        other => other,
    });
    let mut out = String::new();
    html::push_html(&mut out, events);
    out
}

/// The current feature's planning documents that exist, in reading order.
#[must_use]
pub fn of_feature(path: &Path, feature: Option<&str>) -> Vec<Document> {
    let format = crate::format::at(path);
    let Some(snap) = format.open(path, feature) else {
        return Vec::new();
    };
    let Some(f) = snap.feature.as_ref() else {
        return Vec::new();
    };
    let root = Path::new(&snap.root);
    [
        "spec",
        "plan",
        "tasks",
        "research",
        "data-model",
        "quickstart",
    ]
    .iter()
    .filter_map(|name| {
        let rel = format!("{}/{name}.md", f.feature.dir);
        std::fs::read_to_string(root.join(&rel))
            .ok()
            .map(|text| Document {
                name: (*name).to_owned(),
                path: rel,
                html: render(&text),
            })
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_renders_and_raw_html_is_text() {
        let html = render("# Spec\n\n- [ ] T001 a\n\n<script>alert(1)</script>\n\nok <b>x</b>\n");
        assert!(html.contains("<h1>Spec</h1>"), "{html}");
        assert!(html.contains("type=\"checkbox\""), "{html}");
        assert!(
            !html.contains("<script>") && html.contains("&lt;script&gt;"),
            "{html}"
        );
        assert!(!html.contains("<b>"), "{html}");
    }
}
