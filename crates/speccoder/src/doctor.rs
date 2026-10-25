//! `speccoder doctor` as text, and `doctor --bug-report` (F-057): the
//! checks in Markdown, ready for an issue, with secrets and home
//! directories taken out.

use speccoder_core::{CommandData, CommandResult, DoctorLevel};

/// Replaces values that look like secrets and the home directory.
#[must_use]
pub fn redact(text: &str) -> String {
    let home = std::env::var("HOME").ok().filter(|h| h.len() > 1);
    let mut out = String::with_capacity(text.len());
    for word in
        text.split_inclusive(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '=')
    {
        let core =
            word.trim_end_matches(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '=');
        let secret = [
            "sk-ant-",
            "sk-proj-",
            "sk-",
            "ghp_",
            "github_pat_",
            "AKIA",
            "xoxb-",
            "glpat-",
        ]
        .iter()
        .any(|p| core.starts_with(p) && core.len() > p.len() + 8);
        if secret {
            out.push_str("[redacted]");
            out.push_str(&word[core.len()..]);
        } else {
            out.push_str(word);
        }
    }
    match home {
        Some(h) => out.replace(&h, "~"),
        None => out,
    }
}

/// The bug report: versions, every check, and the configuration.
#[must_use]
pub fn bug_report(result: &CommandResult) -> String {
    let Some(CommandData::Doctor(d)) = result.data() else {
        return redact(result.message());
    };
    let mut out = format!(
        "### speccoder doctor\n\n- speccoder {}\n- {}\n\n| Check | Result | Detail |\n|---|---|---|\n",
        d.speccoder, d.os
    );
    for c in &d.checks {
        let level = match c.level {
            DoctorLevel::Pass => "ok",
            DoctorLevel::Info => "optional",
            DoctorLevel::Warn => "warning",
            DoctorLevel::Fail => "**fail**",
        };
        out.push_str(&format!(
            "| {} | {level}{} | {} |\n",
            c.check,
            c.code
                .as_deref()
                .map(|c| format!(" `{c}`"))
                .unwrap_or_default(),
            c.detail.replace('|', "\\|").replace('\n', " ")
        ));
    }
    let config = std::fs::read_to_string("speccoder.toml").ok();
    if let Some(text) = config {
        out.push_str("\n<details><summary>speccoder.toml</summary>\n\n```toml\n");
        out.push_str(text.trim_end());
        out.push_str("\n```\n\n</details>\n");
    }
    redact(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_and_home_are_redacted() {
        let home = std::env::var("HOME").unwrap_or_default();
        let text = format!(
            "key sk-ant-api03-abcdefghijklmnop at {home}/x ghp_0123456789abcdefghij \"sk-proj-aaaaaaaaaaaaaaaa\""
        );
        let r = redact(&text);
        assert!(
            !r.contains("abcdefghijklmnop") && !r.contains("0123456789abcdef"),
            "{r}"
        );
        assert!(!r.contains("sk-proj-aaaa"), "{r}");
        if home.len() > 1 {
            assert!(!r.contains(&home) && r.contains("~/x"), "{r}");
        }
        assert!(r.contains("[redacted]"));
        assert_eq!(redact("sk-short"), "sk-short");
    }
}
