//! What went wrong, as one block of plain text to paste into an issue or a
//! chat: the code and the message, each fix line, the docs link, and
//! speccoder's version, the OS and the run — never a secret. The app
//! copies the same block (its twin in `ui/src/lib/copy.ts`); the CLI's
//! `--copy` puts it on the system clipboard.

use std::io::Write as _;
use std::process::{Command, Stdio};

use speccoder_core::CommandResult;

/// The block for `result`; `run` is the run it happened in, if any.
#[must_use]
pub fn block(result: &CommandResult, run: Option<&str>) -> String {
    let mut out = match result.code() {
        Some(code) => format!(
            "{}[{}]: {}\n",
            code.kind.prefix(),
            code.name,
            result.message()
        ),
        None => format!("{}\n", result.message()),
    };
    for f in result.fix() {
        out.push_str(&format!("fix: {}\n", f.trim_start_matches("→ ")));
    }
    if let Some(code) = result.code() {
        out.push_str(&format!("docs: {}\n", code.docs()));
    }
    out.push_str(&format!(
        "speccoder {} · {} {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    if let Some(run) = run.map(str::to_owned).or_else(|| run_of(result)) {
        out.push_str(&format!(" · run {run}"));
    }
    out.push('\n');
    crate::doctor::redact(&out)
}

/// The run a result's payload names (`data.run_id`).
fn run_of(result: &CommandResult) -> Option<String> {
    let v = serde_json::to_value(result).ok()?;
    v["data"]["run_id"]
        .as_str()
        .or_else(|| v["data"]["run"].as_str())
        .filter(|r| !r.is_empty())
        .map(str::to_owned)
}

/// The clipboard programs tried, in order.
const CLIPBOARDS: &[(&str, &[&str])] = &[
    ("pbcopy", &[]),
    ("wl-copy", &[]),
    ("xclip", &["-selection", "clipboard"]),
];

/// Puts `text` on the system clipboard; the program that took it.
///
/// # Errors
///
/// No clipboard program is installed, or none took the text.
pub fn to_clipboard(text: &str) -> Result<&'static str, String> {
    for (program, args) in CLIPBOARDS {
        let Ok(mut child) = Command::new(program)
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue;
        };
        let wrote = child
            .stdin
            .take()
            .is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
        if wrote && child.wait().is_ok_and(|s| s.success()) {
            return Ok(program);
        }
    }
    Err("no clipboard program took it (pbcopy, wl-copy or xclip)".to_owned())
}

#[cfg(test)]
mod tests {
    use speccoder_core::CommandStatus;

    use super::*;

    #[test]
    fn the_block_names_the_code_its_fixes_and_where_it_happened() {
        let r = CommandResult::new("run", CommandStatus::ConfigError, "no API key")
            .with_code("KEY-MISSING")
            .with_fix(vec![
                "export ANTHROPIC_API_KEY=…".to_owned(),
                "→ or set it in Settings".to_owned(),
            ]);
        let b = block(&r, Some("r-42"));
        let lines: Vec<&str> = b.lines().collect();
        assert_eq!(lines[0], "error[KEY-MISSING]: no API key");
        assert_eq!(lines[1], "fix: export ANTHROPIC_API_KEY=…");
        assert_eq!(lines[2], "fix: or set it in Settings");
        assert!(lines[3].starts_with("docs: https://"), "{b}");
        assert!(
            lines[4].starts_with(&format!("speccoder {}", env!("CARGO_PKG_VERSION")))
                && lines[4].ends_with(" · run r-42"),
            "{b}"
        );
    }

    #[test]
    fn a_masked_value_stays_masked() {
        crate::secrets::register("COPY_TEST_TOKEN", "copy-block-Zq9 value");
        let message = crate::secrets::mask("the provider said: copy-block-Zq9 value is wrong");
        let r = CommandResult::new("run", CommandStatus::RunFailed, &message)
            .with_fix(vec!["sk-ant-api03-abcdefghijklmnopq".to_owned()]);
        let b = block(&r, None);
        assert!(!b.contains("copy-block-Zq9"), "{b}");
        assert!(b.contains("[redacted:COPY_TEST_TOKEN]"), "{b}");
        assert!(!b.contains("abcdefghijklmnopq"), "{b}");
    }
}
