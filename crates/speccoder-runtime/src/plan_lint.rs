//! Checks of a plan before it runs: where the task list will
//! produce no evidence. Deterministic, text-level, warnings only — they
//! never block, delay or re-order a run, and never edit the plan.
//!
//! [`RULES`] is the one source of the three rules: the lints' codes and
//! titles, and the sentence the `speccoder` Spec Kit preset's tasks
//! template states (a test holds the template to it).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use speccoder_core::snapshot::ProjectSnapshot;

use crate::format::{Adapter, PhaseKind, PlanStructure};

/// One rule, as a lint and as the preset says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    /// The lint's code.
    pub code: &'static str,
    /// The sentence the preset's tasks template states.
    pub preset: &'static str,
}

/// The three rules.
pub const RULES: [Rule; 3] = [
    Rule {
        code: "PLAN-BEHAVIOUR-EARLY",
        preset: "Implement a story's behaviour inside that story's phase: Setup and Foundational tasks only create structure — manifests, empty modules, signatures that do not yet behave.",
    },
    Rule {
        code: "PLAN-TEST-ONLY-TASKS",
        preset: "Write no task whose only output is tests: speccoder writes each story's acceptance tests from its scenarios and judges the implementing tasks by them.",
    },
    Rule {
        code: "PLAN-TESTS-UNCOLLECTED",
        preset: "When a task names a test file, name a path the project's test runner collects (Cargo: `tests/<name>.rs`, or `tests/<dir>/main.rs` and the modules it declares; pytest: `test_*.py` under `testpaths`; vitest/jest: a file matching `include`, such as `<name>.test.ts`; Go: `<name>_test.go` beside the package's sources).",
    },
];

/// Who does a task, as the preset states it: design documents are the
/// plan's, a person's steps are labelled (lanes: `speccoder_core::lane`).
pub const LANE_RULES: [&str; 2] = [
    "Design documents — contracts, the data model, research, a quickstart — belong to the plan: write them while planning, not as tasks (a task naming only paths in the feature's folder runs as planning work).",
    "Label a step only a person can take — create an account, set a secret, publish — with `[YOU]` after the ID and markers (`- [ ] T019 [YOU] Create the GitHub repository`): speccoder does not attempt it and asks the person instead.",
];

/// One warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanLint {
    /// Its code.
    pub code: &'static str,
    /// The tasks it names.
    pub tasks: Vec<String>,
    /// What it found and what will happen.
    pub detail: String,
    /// The ways out (commands or `→` lines).
    pub fix: Vec<String>,
    /// For `PLAN-BEHAVIOUR-EARLY`: the task and the story the acceptance
    /// could be written at.
    pub acceptance_at: Option<(String, String)>,
}

/// The tasks joined: `T002, T005, T006`.
fn listed(ids: &[String]) -> String {
    ids.join(", ")
}

/// The re-plan fix: the format's task skill while nothing is ticked.
fn replan(adapter: &dyn Adapter, any_ticked: bool) -> String {
    let skill = (adapter.planning().toward_tasks)(true)
        .last()
        .copied()
        .unwrap_or("tasks");
    if any_ticked {
        "→ re-planning now would rewrite the tasks your ticks are bound to — apply it to the next feature".to_owned()
    } else {
        format!("speccoder spec {skill}  (re-plan with the spec tool)")
    }
}

const KEYWORDS: &[&str] = &[
    "fn",
    "pub",
    "let",
    "mut",
    "impl",
    "struct",
    "enum",
    "trait",
    "return",
    "self",
    "Self",
    "str",
    "String",
    "bool",
    "char",
    "usize",
    "isize",
    "u8",
    "u16",
    "u32",
    "u64",
    "i8",
    "i16",
    "i32",
    "i64",
    "f32",
    "f64",
    "Vec",
    "Option",
    "Result",
    "Box",
    "def",
    "class",
    "async",
    "await",
    "const",
    "var",
    "func",
    "int",
    "float",
    "string",
    "number",
    "void",
    "null",
    "None",
    "true",
    "false",
    "new",
    "type",
    "interface",
    "export",
    "import",
    "from",
    "use",
    "mod",
    "crate",
];

/// The symbols a code span names: a lone identifier, or one called
/// (`greet(…)`) — never a path, a parameter, a keyword or a type word.
fn symbols(span: &str) -> Vec<String> {
    let s = span.trim();
    if s.contains('/') || (s.contains('.') && !s.contains('(')) {
        return Vec::new();
    }
    let ident = |w: &str| {
        w.len() >= 3
            && w.chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
            && w.chars().all(|c| c.is_alphanumeric() || c == '_')
            && !KEYWORDS.contains(&w)
    };
    if ident(s) {
        return vec![s.to_owned()];
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_alphabetic() || chars[i] == '_' {
            let st = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let w: String = chars[st..i].iter().collect();
            if chars.get(i) == Some(&'(') && ident(&w) {
                out.push(w);
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Whether a task only checks (its verb): it implements nothing.
fn only_checks(title: &str) -> bool {
    let first = title
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        first.as_str(),
        "verify" | "confirm" | "check" | "review" | "ensure" | "validate" | "run" | "document"
    )
}

/// Every lint over the selected unit of `snapshot` at `root`.
#[must_use]
pub fn lints(adapter: &dyn Adapter, snapshot: &ProjectSnapshot, root: &Path) -> Vec<PlanLint> {
    let Some(tasks) = snapshot.feature.as_ref().and_then(|f| f.tasks.as_ref()) else {
        return Vec::new();
    };
    let any_ticked = tasks.tasks.iter().any(|t| t.done);
    let mut out = Vec::new();
    if let Some(l) = uncollected(adapter, tasks, root, any_ticked) {
        out.push(l);
    }
    if let Some(structure) = adapter.plan_structure(snapshot) {
        out.extend(behaviour_early(adapter, &structure, any_ticked));
        if let Some(l) = test_only(adapter, &structure, any_ticked) {
            out.push(l);
        }
    }
    out
}

/// `PLAN-TESTS-UNCOLLECTED`: test paths the tasks name that the runner's
/// static rule would not collect (unless another task creates the file
/// declaring them), and test files in the tree nothing runs.
fn uncollected(
    adapter: &dyn Adapter,
    tasks: &speccoder_core::snapshot::ParsedTasks,
    root: &Path,
    any_ticked: bool,
) -> Option<PlanLint> {
    let files = crate::collection::static_cfg::files(root);
    let packages = crate::collection::static_cfg::rust_packages(root, &files);
    let named: Vec<(String, Vec<String>)> = tasks
        .tasks
        .iter()
        .map(|t| (t.id.clone(), speccoder_core::scope::named_paths(&t.title)))
        .collect();
    let all_named: BTreeSet<&str> = named
        .iter()
        .flat_map(|(_, p)| p.iter().map(String::as_str))
        .collect();
    // By directory: the tasks naming paths under it, and whether Cargo's rule.
    let mut by_dir: BTreeMap<String, (Vec<String>, bool)> = BTreeMap::new();
    for (id, paths) in &named {
        for p in paths {
            let is_dir = p.ends_with('/');
            let path = p.trim_end_matches('/');
            if !crate::tools::paths::is_test_path(&format!(
                "{path}{}",
                if is_dir { "/x" } else { "" }
            )) {
                continue;
            }
            let rust_pkg = crate::collection::static_cfg::owner(&packages, path);
            let (dir, never, rust) = if path.ends_with(".rs") || (is_dir && rust_pkg.is_some()) {
                let Some(pkg) = rust_pkg else { continue };
                let tests =
                    speccoder_core::collection::join(&pkg.root, "tests").unwrap_or_default();
                let Some(rest) = path.strip_prefix(&format!("{tests}/")) else {
                    continue;
                };
                let parts: Vec<&str> = rest.split('/').collect();
                let first = parts.first().copied().unwrap_or_default();
                // tests/<x>.rs, or tests/<dir>/main.rs: collected by the rule.
                if (!is_dir && parts.len() == 1) || (parts.len() == 2 && parts[1] == "main.rs") {
                    continue;
                }
                let main = format!("{tests}/{first}/main.rs");
                let declared = files.contains(&main)
                    || all_named.contains(main.as_str())
                    || files.contains(&format!("{tests}/{first}.rs"));
                (
                    format!("{tests}/{first}/"),
                    !(declared && pkg.autotests),
                    true,
                )
            } else if is_dir {
                continue;
            } else {
                let never = crate::collection::statically_refused(root, path, "").is_some();
                (
                    path.rsplit_once('/')
                        .map_or(String::new(), |(d, _)| format!("{d}/")),
                    never,
                    false,
                )
            };
            if never {
                let e = by_dir.entry(dir).or_insert((Vec::new(), rust));
                if !e.0.contains(id) {
                    e.0.push(id.clone());
                }
            }
        }
    }
    let layout = crate::layout::detect(root, &crate::verify::check_argvs(root));
    let dead: Vec<String> = layout
        .runners
        .iter()
        .flat_map(|r| r.not_run.clone())
        .collect();
    if by_dir.is_empty() && dead.is_empty() {
        return None;
    }
    let mut tasks_named: Vec<String> = Vec::new();
    let mut sentences = Vec::new();
    let mut fix = Vec::new();
    for (dir, (ids, rust)) in &by_dir {
        for id in ids {
            if !tasks_named.contains(id) {
                tasks_named.push(id.clone());
            }
        }
        sentences.push(format!(
            "{} name `{dir}` — {}",
            listed(ids),
            if *rust {
                "Cargo won't build the tests there"
            } else {
                "the runner won't collect tests there"
            }
        ));
        if *rust {
            let tests = dir
                .trim_end_matches('/')
                .rsplit_once('/')
                .map_or("tests", |(t, _)| t);
            fix.push(format!("→ rename to `{tests}/<name>.rs`"));
            fix.push(format!("→ add a task that creates `{dir}main.rs`"));
        } else {
            fix.push("→ name a path the runner collects (the test layout in `speccoder doctor` says where)".to_owned());
        }
    }
    if !dead.is_empty() {
        sentences.push(format!("test files nothing runs: {}", dead.join(", ")));
    }
    tasks_named.sort();
    fix.push(replan(adapter, any_ticked));
    fix.dedup();
    Some(PlanLint {
        code: "PLAN-TESTS-UNCOLLECTED",
        tasks: tasks_named,
        detail: sentences.join("; "),
        fix,
        acceptance_at: None,
    })
}

/// `PLAN-BEHAVIOUR-EARLY`: a Setup or Foundational task that names, as
/// code, a symbol a story names in its scenarios or tasks.
fn behaviour_early(adapter: &dyn Adapter, s: &PlanStructure, any_ticked: bool) -> Vec<PlanLint> {
    let mut story_symbols: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for (story, text) in &s.stories {
        let set = story_symbols.entry(story.as_str()).or_default();
        for span in crate::format::code_spans(text) {
            set.extend(symbols(&span));
        }
    }
    for t in s.tasks.iter().filter(|t| t.phase == PhaseKind::Story) {
        if let Some(st) = &t.story {
            let set = story_symbols.entry(st.as_str()).or_default();
            for span in &t.spans {
                set.extend(symbols(span));
            }
        }
    }
    let mut out = Vec::new();
    for t in s
        .tasks
        .iter()
        .filter(|t| matches!(t.phase, PhaseKind::Setup | PhaseKind::Foundational))
        .filter(|t| !only_checks(&t.title))
    {
        let mine: BTreeSet<String> = t.spans.iter().flat_map(|sp| symbols(sp)).collect();
        let Some((story, shared)) = story_symbols.iter().find_map(|(st, set)| {
            let shared: Vec<&String> = mine.intersection(set).collect();
            (!shared.is_empty()).then_some((*st, shared))
        }) else {
            continue;
        };
        let phase = if t.phase == PhaseKind::Setup {
            "Setup"
        } else {
            "Foundational"
        };
        let names = shared
            .iter()
            .map(|n| format!("`{n}`"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push(PlanLint {
            code: "PLAN-BEHAVIOUR-EARLY",
            tasks: vec![t.id.clone()],
            detail: format!(
                "{} ({phase}) names {names}, which {story} is about — if {} implements it, {story}'s acceptance cannot go red and its tasks will tick regression-only",
                t.id, t.id
            ),
            fix: vec![
                format!("speccoder task acceptance-at {} --story {story}  (write {story}'s acceptance tests at {})", t.id, t.id),
                replan(adapter, any_ticked),
            ],
            acceptance_at: Some((t.id.clone(), story.to_owned())),
        });
    }
    out
}

/// `PLAN-TEST-ONLY-TASKS`: story tasks whose only named outputs are test
/// paths, beside an implementing task of the same story.
fn test_only(adapter: &dyn Adapter, s: &PlanStructure, any_ticked: bool) -> Option<PlanLint> {
    let is_test_only = |t: &crate::format::PlannedTask| {
        let paths = speccoder_core::scope::named_paths(&t.title);
        let verb = t
            .title
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        !paths.is_empty()
            && paths.iter().all(|p| {
                let p = p.trim_end_matches('/');
                crate::tools::paths::is_test_path(p)
                    || crate::tools::paths::is_test_path(&format!("{p}/x"))
            })
            && matches!(verb.as_str(), "add" | "write" | "create")
            && t.title.to_ascii_lowercase().contains("test")
    };
    let mut ids = Vec::new();
    for t in s
        .tasks
        .iter()
        .filter(|t| t.story.is_some() && is_test_only(t))
    {
        let implementing = s
            .tasks
            .iter()
            .any(|o| o.story == t.story && o.id != t.id && !is_test_only(o));
        if implementing {
            ids.push(t.id.clone());
        }
    }
    (!ids.is_empty()).then(|| PlanLint {
        code: "PLAN-TEST-ONLY-TASKS",
        detail: format!(
            "{} only write tests — speccoder writes each story's acceptance tests itself; merge them into the implementing tasks",
            listed(&ids)
        ),
        tasks: ids,
        fix: vec![replan(adapter, any_ticked)],
        acceptance_at: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_are_called_names_or_lone_identifiers() {
        assert_eq!(symbols("greet(name: &str) -> String"), ["greet"]);
        assert_eq!(symbols("greet"), ["greet"]);
        assert!(symbols("src/lib.rs").is_empty());
        assert!(symbols("Hello, <name>!").is_empty());
        assert!(symbols("String").is_empty());
    }

    #[test]
    fn the_preset_template_states_every_rule() {
        let template = include_str!("../assets/preset/speccoder/templates/tasks-template.md");
        for r in RULES {
            assert!(
                template.contains(r.preset),
                "{} is not in the preset",
                r.code
            );
            assert!(
                speccoder_core::codes::lookup(r.code).is_some(),
                "{}",
                r.code
            );
        }
        for rule in LANE_RULES {
            assert!(template.contains(rule), "{rule} is not in the preset");
        }
        assert!(
            template.contains("- **[YOU]**:"),
            "the format line names [YOU]"
        );
    }
}
