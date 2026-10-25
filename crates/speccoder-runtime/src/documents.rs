//! A feature's planning documents, rendered for an interface: the
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

/// Renders Markdown; raw HTML becomes escaped text. No heading attributes
/// (`# x {style=…}`): a file the model wrote would set styles and ids on the
/// page that shows it.
#[must_use]
pub fn render(markdown: &str) -> String {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH;
    let events = Parser::new_ext(markdown, options).map(|e| match e {
        Event::Html(t) | Event::InlineHtml(t) => Event::Text(t),
        other => other,
    });
    let mut out = String::new();
    html::push_html(&mut out, events);
    out
}

/// The current feature's planning documents that exist, in reading order:
/// (name, path relative to the project, text).
#[must_use]
pub fn sources(path: &Path, feature: Option<&str>) -> Vec<(String, String, String)> {
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
            .map(|text| ((*name).to_owned(), rel, text))
    })
    .collect()
}

/// The current feature's planning documents, rendered.
#[must_use]
pub fn of_feature(path: &Path, feature: Option<&str>) -> Vec<Document> {
    sources(path, feature)
        .into_iter()
        .map(|(name, path, text)| Document {
            name,
            path,
            html: render(&text),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_cannot_style_or_script_the_page() {
        let html = render(
            "# Hi {style=position:fixed;inset:0 #top .x}\n\n<script>alert(1)</script>\n\n<b onclick=x>b</b>\n",
        );
        // The heading has no attributes; the braces stay text.
        assert!(html.starts_with("<h1>Hi {style="), "{html}");
        assert!(!html.contains("<h1 "), "{html}");
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("<b "), "{html}");
    }

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
