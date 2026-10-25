//! `speccoder explain CODE`: a code's full entry, offline, and the
//! codes page of the documentation, generated from the same catalog.

use speccoder_core::codes::{self, CATALOG};
use speccoder_core::{CodeInfo, CommandData, CommandResult, CommandStatus, ExplainReport};

/// `explain [CODE] [--list]`.
#[must_use]
pub fn command(code: Option<&str>, list: bool) -> CommandResult {
    let report = |codes: Vec<CodeInfo>| CommandData::Explain(Box::new(ExplainReport { codes }));
    match (code, list) {
        (_, true) | (None, false) => {
            CommandResult::ok("explain", &format!("{} codes", CATALOG.len()))
                .with_data(report(CATALOG.iter().map(CodeInfo::of).collect()))
        }
        (Some(name), false) => match codes::lookup(&name.to_ascii_uppercase()) {
            Some(e) => {
                CommandResult::ok("explain", e.name).with_data(report(vec![CodeInfo::of(e)]))
            }
            None => {
                let near = codes::nearest(name, 3);
                let message = if near.is_empty() {
                    format!("no code `{name}`")
                } else {
                    format!("no code `{name}`; did you mean {}?", near.join(", "))
                };
                let mut fix: Vec<String> = near
                    .iter()
                    .map(|n| format!("speccoder explain {n}"))
                    .collect();
                fix.push("speccoder explain --list".to_owned());
                CommandResult::new("explain", CommandStatus::UsageError, &message)
                    .with_code("USAGE-UNKNOWN-CODE")
                    .with_fix(fix)
            }
        },
    }
}

/// One entry as text: what `explain` prints and the codes page shows.
pub fn entry(w: &mut dyn std::io::Write, c: &CodeInfo) -> std::io::Result<()> {
    writeln!(w, "{} — {}", c.code, c.title)?;
    writeln!(
        w,
        "  {} · exit {} ({})",
        c.kind_word(),
        c.exit_code,
        serde_json::to_value(c.status)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    )?;
    writeln!(w)?;
    for line in textwrap(&c.explain, 96) {
        writeln!(w, "  {line}")?;
    }
    writeln!(w)?;
    for f in &c.fix {
        writeln!(w, "  fix: {f}")?;
    }
    writeln!(w, "  docs: {}", c.docs)
}

trait KindWord {
    fn kind_word(&self) -> &'static str;
}

impl KindWord for CodeInfo {
    fn kind_word(&self) -> &'static str {
        match self.kind {
            codes::Kind::Error => "error",
            codes::Kind::Stop => "stop (waits for a person)",
            codes::Kind::Warning => "warning (it worked, with less evidence)",
        }
    }
}

fn textwrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The documentation's codes page (`site/content/docs/codes.md`).
#[cfg(test)]
#[must_use]
pub fn page() -> String {
    let mut out = String::from(
        "+++\ntitle = \"Codes\"\ndescription = \"Every code speccoder prints with an error, a stop or a warning: what it means, why it happens and how to fix it.\"\nweight = 135\n\n[extra]\ngroup = \"Reference\"\n+++\n\n<!-- Generated from crates/speccoder-core/src/codes.rs by `SPECCODER_BLESS=1 cargo nextest run -p speccoder --test contract`; do not edit by hand. -->\n\nEvery result that is not plainly `ok` names a code — `error[CODE]`, `stopped[CODE]` (the run waits for you) or `warning[CODE]` (it worked, with less evidence) — with a `fix:` line and a link here. In JSON the same is `code`, `fix` and `docs` on the [result envelope](@/docs/cli.md#json-output). `speccoder explain CODE` prints an entry offline.\n\n| Code | Kind | Exit | Means |\n|---|---|---|---|\n",
    );
    for e in CATALOG {
        let c = CodeInfo::of(e);
        out.push_str(&format!(
            "| [`{}`](#{}) | {} | {} | {} |\n",
            c.code,
            c.code.to_ascii_lowercase(),
            c.kind_word().split(' ').next().unwrap_or_default(),
            c.exit_code,
            c.title
        ));
    }
    for e in CATALOG {
        let c = CodeInfo::of(e);
        out.push_str(&format!(
            "\n### {}\n\n{}.\n\n{}\n\n",
            c.code,
            capitalised(&c.title),
            c.explain
        ));
        for f in &c.fix {
            match f.strip_prefix("→ ") {
                Some(action) => out.push_str(&format!("- {}\n", capitalised(action))),
                None => out.push_str(&format!("- `{f}`\n")),
            }
        }
    }
    out
}

#[cfg(test)]
fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_code_is_explained_and_a_typo_gets_suggestions() {
        let r = command(Some("needs-approval"), false);
        assert_eq!(r.status(), CommandStatus::Ok);
        let Some(CommandData::Explain(e)) = r.data() else {
            panic!()
        };
        assert_eq!(e.codes[0].code, "NEEDS-APPROVAL");
        let r = command(Some("NEEDS-APROVAL"), false);
        assert_eq!(r.status(), CommandStatus::UsageError);
        assert_eq!(r.code().map(|c| c.name), Some("USAGE-UNKNOWN-CODE"));
        assert!(r.message().contains("NEEDS-APPROVAL"), "{}", r.message());
        assert_eq!(r.fix()[0], "speccoder explain NEEDS-APPROVAL");
    }

    #[test]
    fn every_fix_is_a_command_or_an_action() {
        let programs = [
            "speccoder ",
            "git ",
            "gh ",
            "specify ",
            "openspec ",
            "export ",
        ];
        for e in CATALOG {
            for f in e.fix {
                assert!(
                    f.starts_with("→ ") || programs.iter().any(|p| f.starts_with(p)),
                    "{}: {f}",
                    e.name
                );
                assert_eq!(
                    f.matches('"').count() % 2,
                    0,
                    "{}: unbalanced quote in {f}",
                    e.name
                );
            }
        }
    }
}
