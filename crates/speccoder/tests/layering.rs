//! The workspace layering rules (core is pure, only the runtime executes), checked
//! over the resolved normal-dependency graph from `cargo metadata`.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use serde_json::Value;

const CORE: &str = "speccoder-core";
const SPECKIT: &str = "speccoder-speckit";
const OPENSPEC: &str = "speccoder-openspec";
const RUNTIME: &str = "speccoder-runtime";
const WEB: &str = "speccoder-web";
const BIN: &str = "speccoder";

/// Crates the pure core must never reach, directly or transitively (rule 5).
const CORE_DENY: &[&str] = &[
    "tokio",
    "async-std",
    "smol",
    "futures",
    "reqwest",
    "hyper",
    "ureq",
    "mio",
    "socket2",
    "walkdir",
    "ignore",
    "rusqlite",
    "sqlx",
    "redb",
    "agentplane",
    "git2",
    "gix",
    "duct",
    "nix",
    "libc",
];

/// Package name → names of its normal (non-dev, non-build) dependencies.
type Graph = BTreeMap<String, BTreeSet<String>>;

fn graph() -> Graph {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let meta: Value = serde_json::from_slice(&out.stdout).unwrap();

    // Workspace crates by name; every other package by name and version, so
    // two versions of one crate (Tauri's older schemars beside core's) are
    // two nodes, and one's dependencies are not blamed on the other.
    let members: BTreeSet<&str> = meta["workspace_members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap())
        .collect();
    let names: BTreeMap<&str, String> = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let id = p["id"].as_str().unwrap();
            let name = p["name"].as_str().unwrap();
            let key = if members.contains(id) {
                name.to_owned()
            } else {
                format!("{name}@{}", p["version"].as_str().unwrap())
            };
            (id, key)
        })
        .collect();

    let mut graph = Graph::new();
    for node in meta["resolve"]["nodes"].as_array().unwrap() {
        let from = names[node["id"].as_str().unwrap()].clone();
        let deps = node["deps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| {
                d["dep_kinds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|k| k["kind"].is_null())
            })
            .map(|d| names[d["pkg"].as_str().unwrap()].clone())
            .collect();
        graph
            .entry(from)
            .or_default()
            .extend::<BTreeSet<String>>(deps);
    }
    graph
}

/// A node's crate name, without its version.
fn base(node: &str) -> &str {
    node.split('@').next().unwrap_or(node)
}

fn direct<'g>(g: &'g Graph, krate: &str) -> &'g BTreeSet<String> {
    g.get(krate)
        .unwrap_or_else(|| panic!("layering: {krate} is not in the workspace"))
}

fn closure(g: &Graph, krate: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![krate.to_owned()];
    while let Some(k) = stack.pop() {
        for d in g.get(&k).into_iter().flatten() {
            if seen.insert(d.clone()) {
                stack.push(d.clone());
            }
        }
    }
    seen
}

/// Workspace crates `krate` may depend on directly (rules 1–4).
fn allowed_workspace_deps(krate: &str) -> &'static [&'static str] {
    match krate {
        CORE => &[],
        SPECKIT | OPENSPEC => &[CORE],
        RUNTIME => &[CORE, SPECKIT, OPENSPEC],
        // The web host serves the runtime's payloads; it reads no format.
        WEB => &[CORE, RUNTIME],
        BIN => &[CORE, SPECKIT, OPENSPEC, RUNTIME, WEB],
        _ => &[],
    }
}

fn violations(g: &Graph) -> Vec<String> {
    let workspace = [CORE, SPECKIT, OPENSPEC, RUNTIME, WEB, BIN];
    let mut errors = Vec::new();

    for (rule, krate) in [
        (1, CORE),
        (2, SPECKIT),
        (2, OPENSPEC),
        (3, RUNTIME),
        (4, WEB),
        (4, BIN),
    ] {
        let allowed = allowed_workspace_deps(krate);
        for dep in direct(g, krate) {
            if workspace.contains(&dep.as_str()) && !allowed.contains(&dep.as_str()) {
                errors.push(format!(
                    "layering: {krate} must not depend on {dep} (rule {rule}: allowed workspace dependencies are {allowed:?})"
                ));
            }
        }
    }

    for dep in closure(g, CORE) {
        if CORE_DENY.contains(&base(&dep)) {
            errors.push(format!(
                "layering: {CORE} must not depend on {dep} (rule 5: the core stays free of IO and async crates)"
            ));
        }
    }

    for (krate, deps) in g {
        if krate != RUNTIME && deps.iter().any(|d| base(d) == "agentplane") {
            errors.push(format!(
                "layering: {krate} must not depend on agentplane (rule 6: only {RUNTIME} embeds the substrate)"
            ));
        }
    }
    errors
}

#[test]
fn workspace_respects_the_layering_rules() {
    let errors = violations(&graph());
    assert!(errors.is_empty(), "\n{}", errors.join("\n"));
}

#[test]
fn rules_reject_forbidden_edges() {
    let mut g = graph();
    g.get_mut(SPECKIT).unwrap().insert(RUNTIME.to_owned());
    g.get_mut(OPENSPEC).unwrap().insert(SPECKIT.to_owned());
    g.get_mut(CORE).unwrap().insert("tokio".to_owned());
    g.get_mut(BIN).unwrap().insert("agentplane".to_owned());
    let errors = violations(&g).join("\n");
    assert!(
        errors.contains("speccoder-speckit must not depend on speccoder-runtime (rule 2"),
        "{errors}"
    );
    assert!(
        errors.contains("speccoder-openspec must not depend on speccoder-speckit (rule 2"),
        "{errors}"
    );
    assert!(
        errors.contains("speccoder-core must not depend on tokio (rule 5"),
        "{errors}"
    );
    assert!(
        errors.contains("speccoder must not depend on agentplane (rule 6"),
        "{errors}"
    );
}

/// Spec Kit's paths, files and commands, as string contents: only the Spec
/// Kit adapter (`speccoder-runtime/src/format/speckit/`) may name them
///. `specs` matches a whole string only.
const SPECKIT_LITERALS: &[&str] = &[
    ".specify",
    "specs/",
    "User Story",
    "speckit",
    "specify init",
    "specify workflow",
    "SPECIFY_",
    "tasks.md",
    "spec.md",
    "plan.md",
    ".specify/bugs",
    "assessment.md",
    "bugfixes",
    "bugfix",
];

/// Spec Kit's parser crate, which only the adapter calls.
const SPECKIT_CRATE: &str = "speccoder_speckit";

/// The (line, literal) pairs of `text`: Spec Kit literals inside string
/// literals and the parser crate anywhere in code; comments and everything
/// from the first `#[cfg(test)]` are skipped.
fn scan(text: &str) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    let mut in_string = false;
    let mut string = String::new();
    let mut start = 0;
    for (i, line) in text.lines().enumerate() {
        if !in_string && line.trim_start().starts_with("#[cfg(test)]") {
            break;
        }
        let mut code = String::new();
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if in_string {
                match c {
                    '\\' => {
                        // An escape, or a line continuation at the end.
                        if let Some(n) = chars.next() {
                            string.push(n);
                        }
                    }
                    '"' => {
                        in_string = false;
                        let hits = SPECKIT_LITERALS
                            .iter()
                            .filter(|l| string.contains(*l))
                            .copied()
                            .chain((string == "specs").then_some("\"specs\""));
                        found.extend(hits.map(|l| (start + 1, l)));
                        string.clear();
                    }
                    c => string.push(c),
                }
            } else if c == '/' && chars.peek() == Some(&'/') {
                break;
            } else if c == '"' {
                in_string = true;
                start = i;
            } else if c == '\'' {
                // A char literal (`'"'`), not a lifetime.
                let rest: String = chars.clone().take(3).collect();
                if rest.starts_with("\"'") || rest.starts_with("\\\"'") {
                    let skip = if rest.starts_with('"') { 2 } else { 3 };
                    for _ in 0..skip {
                        chars.next();
                    }
                }
            } else {
                code.push(c);
            }
        }
        if code.contains(SPECKIT_CRATE) {
            found.push((i + 1, SPECKIT_CRATE));
        }
    }
    found
}

fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn runtime_has_no_speckit_literals_outside_the_adapter() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../speccoder-runtime/src");
    // Each format's own adapter names its own files.
    let adapters = [
        src.join("format/speckit"),
        src.join("format/openspec"),
        src.join("format/stubs.rs"),
    ];
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    files.sort();
    let mut errors = Vec::new();
    for f in files
        .iter()
        .filter(|f| !adapters.iter().any(|a| f.starts_with(a)))
    {
        let text = std::fs::read_to_string(f).unwrap();
        let rel = f.strip_prefix(&src).unwrap().display();
        for (line, lit) in scan(&text) {
            errors.push(format!(
                "layering: {rel}:{line} contains Spec Kit literal `{lit}` outside format/speckit"
            ));
        }
    }
    assert!(errors.is_empty(), "\n{}", errors.join("\n"));
}

/// The format tools' names, as a string starts with them: a new project's
/// plan reaches the format's own init only through the adapter seam.
fn format_commands(text: &str) -> Vec<usize> {
    code_lines(text)
        .filter(|(_, l)| {
            ["\"specify", "\"openspec", "\"speckit-", "\"openspec-"]
                .iter()
                .any(|n| l.contains(n))
        })
        .map(|(i, _)| i)
        .collect()
}

/// `text`'s lines before its tests, comments cut, numbered from 1.
fn code_lines(text: &str) -> impl Iterator<Item = (usize, String)> + '_ {
    text.lines()
        .take_while(|l| !l.trim_start().starts_with("#[cfg(test)]"))
        .enumerate()
        .map(|(i, l)| (i + 1, l.split("//").next().unwrap_or_default().to_owned()))
}

#[test]
fn a_new_projects_plan_names_no_format_tool_outside_the_adapter() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../speccoder-runtime/src/new_project");
    let mut files = Vec::new();
    rust_files(&dir, &mut files);
    assert!(!files.is_empty());
    let mut errors = Vec::new();
    for f in &files {
        for line in format_commands(&std::fs::read_to_string(f).unwrap()) {
            errors.push(format!(
                "layering: new_project/{}:{line} names a spec format's tool: it belongs to format/ (Adapter::init_steps)",
                f.file_name().unwrap().to_string_lossy()
            ));
        }
    }
    assert!(errors.is_empty(), "\n{}", errors.join("\n"));
    assert_eq!(
        format_commands("let a = [\"specify\", \"init\"];\n// \"openspec\"\n"),
        [1]
    );
}

#[test]
fn scan_finds_a_planted_literal() {
    let text = "// a comment about .specify/\nuse crate::format::speckit;\nfn f(a: A) {\n    let p = \".specify/memory\"; // tasks.md\n    let q = a.specify_dir;\n    let c = '\"';\n    speccoder_speckit::inspect(\"x \\\n tasks.md\");\n    let d = \"specs\";\n}\n#[cfg(test)]\nmod tests { const X: &str = \"tasks.md\"; }\n";
    assert_eq!(
        scan(text),
        [
            (4, ".specify"),
            (7, "speccoder_speckit"),
            (7, "tasks.md"),
            (9, "\"specs\""),
        ]
    );
}

/// `text` up to its first `#[cfg(test)]`, without line comments.
fn code_of(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        if line.trim_start().starts_with("#[cfg(test)]") {
            break;
        }
        let code = match line.find("//") {
            // `//` inside a string (a URL) is not a comment.
            Some(i) if line[..i].matches('"').count() % 2 == 0 => &line[..i],
            _ => line,
        };
        out.push_str(code);
        out.push('\n');
    }
    out
}

fn workspace_sources() -> Vec<std::path::PathBuf> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for e in std::fs::read_dir(&crates).unwrap().flatten() {
        let src = e.path().join("src");
        if src.is_dir() {
            rust_files(&src, &mut files);
        }
    }
    files.sort();
    files
}

fn relative(f: &Path) -> String {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .unwrap();
    let f = f.canonicalize().unwrap();
    f.strip_prefix(&crates).unwrap().display().to_string()
}

/// The `Runnable` variants each `match` block of `code` names, by the
/// block's line: a block naming most of them maps every kind to something.
fn matches_over_runnables(code: &str) -> Vec<(usize, std::collections::BTreeSet<String>)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = code[from..].find("match ") {
        let start = from + i;
        from = start + 6;
        // A word, not the end of another one (`rematch `).
        if code[..start]
            .chars()
            .last()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            continue;
        }
        let Some(open) = code[start..].find('{').map(|o| start + o) else {
            break;
        };
        let mut depth = 0;
        let mut end = code.len();
        for (j, c) in code[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + j;
                        break;
                    }
                }
                _ => {}
            }
        }
        let block = &code[open..end];
        let names: std::collections::BTreeSet<String> = block
            .match_indices("Runnable::")
            .map(|(k, m)| {
                block[k + m.len()..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric())
                    .collect()
            })
            .collect();
        let line = code[..start].matches('\n').count() + 1;
        out.push((line, names));
    }
    out
}

/// The kinds of `Runnable` (a run with or without a task is one variant).
fn runnable_variants() -> usize {
    speccoder_core::actions::Runnable::examples()
        .iter()
        .map(|r| serde_json::to_value(r).unwrap()["kind"].to_string())
        .collect::<BTreeSet<_>>()
        .len()
}

/// Where a `match` over most `Runnable` kinds is, outside the dispatcher
/// and the core's own definition.
fn second_dispatchers(files: &[(String, String)]) -> Vec<String> {
    let half = runnable_variants() / 2;
    let mut errors = Vec::new();
    for (rel, text) in files {
        if rel == "speccoder-core/src/actions.rs" {
            continue;
        }
        for (line, names) in matches_over_runnables(&code_of(text)) {
            if names.len() >= half && rel != "speccoder-runtime/src/actions.rs" {
                errors.push(format!(
                    "layering: {rel}:{line} matches {} runnable kinds: only `actions::perform` maps kinds to runtime calls",
                    names.len()
                ));
            }
        }
    }
    errors
}

fn sources() -> Vec<(String, String)> {
    workspace_sources()
        .iter()
        .map(|f| (relative(f), std::fs::read_to_string(f).unwrap()))
        .collect()
}

#[test]
fn one_dispatcher_maps_runnable_kinds_to_runtime_calls() {
    let files = sources();
    let errors = second_dispatchers(&files);
    assert!(errors.is_empty(), "\n{}", errors.join("\n"));
    // And it is there: one match over every kind.
    let (_, actions) = files
        .iter()
        .find(|(r, _)| r == "speccoder-runtime/src/actions.rs")
        .unwrap();
    let most = matches_over_runnables(&code_of(actions))
        .into_iter()
        .map(|(_, n)| n.len())
        .max()
        .unwrap_or(0);
    assert_eq!(most, runnable_variants(), "perform names every kind");
    // The offer table decides every pair itself.
    let table = actions
        .split("pub const fn offered(")
        .nth(1)
        .and_then(|t| t.split("\n}\n").next())
        .unwrap();
    assert!(
        !table.contains("_ =>"),
        "the offer table has a wildcard arm"
    );
}

#[test]
fn the_dispatcher_scan_finds_a_planted_second_one() {
    let planted = "fn take(r: &Runnable) {\n    match r {\n        Runnable::Skill { .. } => a(),\n        Runnable::Run { .. } => b(),\n        Runnable::Converge | Runnable::Commit { .. } | Runnable::Approve { .. } => c(),\n        Runnable::Reject { .. } | Runnable::Accept { .. } | Runnable::Skip { .. } => d(),\n        Runnable::Nudge { .. } | Runnable::Rule { .. } => e(),\n        Runnable::Review { .. } | Runnable::IssueComment { .. } | Runnable::QuickChange { .. } | Runnable::Feature => g(),\n        Runnable::OracleTests { .. } | Runnable::ApproveAndRun { .. } | Runnable::Comment { .. } | Runnable::CommentsSend { .. } | Runnable::RevisionUndo { .. } | Runnable::CodeNudge { .. } => h(),\n        _ => f(),\n    }\n}\n// match r { Runnable::Review {} }\n";
    let card = "fn verb(a: &Action) -> &str {\n    match &a.run {\n        Some(Runnable::Commit { .. }) => \"Commit them?\",\n        Some(Runnable::Run { .. } | Runnable::Converge) => \"Start it now?\",\n        _ => \"Run it now?\",\n    }\n}\n";
    let errors = second_dispatchers(&[
        ("speccoder-web/src/jobs.rs".to_owned(), planted.to_owned()),
        ("speccoder/src/follow.rs".to_owned(), card.to_owned()),
    ]);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].starts_with("layering: speccoder-web/src/jobs.rs:2 matches 20"),
        "{errors:?}"
    );
}

/// Paths into the runtime from `text` that do not go through its facade.
fn around_the_facade(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        for (k, _) in code.match_indices("speccoder_runtime::") {
            let rest = &code[k + "speccoder_runtime::".len()..];
            if !rest.starts_with("service") {
                let path: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                    .collect();
                let path = if rest.starts_with('{') {
                    "{…}".to_owned()
                } else {
                    path
                };
                found.push((i + 1, path));
            }
        }
    }
    found
}

#[test]
fn the_web_host_reaches_the_runtime_only_through_its_facade() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../speccoder-web/src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let mut errors = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        for (line, path) in around_the_facade(&text) {
            errors.push(format!(
                "layering: speccoder-web/src/{}:{line} uses speccoder_runtime::{path}: the web host goes through speccoder_runtime::service",
                f.strip_prefix(&src).unwrap().display()
            ));
        }
    }
    assert!(errors.is_empty(), "\n{}", errors.join("\n"));
}

#[test]
fn the_facade_scan_finds_a_planted_path() {
    let text = "use speccoder_runtime::service::actions;\nuse speccoder_runtime::approvals::decide;\n// speccoder_runtime::git is not code\nlet r = speccoder_runtime::controls::skip(a, b, c, d);\nuse speccoder_runtime::{status, service};\n";
    assert_eq!(
        around_the_facade(text),
        [
            (2, "approvals::decide".to_owned()),
            (4, "controls::skip".to_owned()),
            (5, "{…}".to_owned()),
        ]
    );
}

/// Web routes that change something without a `Runnable`: the session
/// (the token is the page's), sign-in, a running job's answers and its
/// stop, and the setup forms that predate one dispatcher. Every other
/// route that writes goes through `perform`.
const WRITES_WITHOUT_A_KIND: &[(&str, &str)] = &[
    ("start", "starts a job: the job runs `actions::perform`"),
    ("answer", "answers what a running job asks"),
    ("stop", "stops the running job"),
    (
        "fix_run",
        "runs a command the host issued as a fix, as the person: not an action of speccoder's",
    ),
    ("terminal", "opens a terminal at the project folder"),
    ("config_save", "the settings form (speccoder.toml)"),
    ("config_commit", "commits speccoder.toml"),
    ("setup_init", "the setup form (speccoder init)"),
    ("setup_commit", "commits what setup wrote"),
    ("github_login", "signs in with a token"),
    ("github_device", "starts a device sign-in"),
    ("github_device_wait", "finishes a device sign-in"),
    (
        "open",
        "binds a host started outside a project to a folder: the host's state, no project write",
    ),
    ("forget", "removes a recent project from speccoder's list"),
    (
        "first_feature",
        "marks the new project's idea as offered once (speccoder's state)",
    ),
    (
        "shape",
        "a body, not a write: one model call proposes, the field's own action writes what the person accepts",
    ),
    (
        "shape_check",
        "a body, not a write: the writing check, no model",
    ),
    (
        "shape_judge",
        "a body, not a write: the changed proposal judged by code",
    ),
];

/// Handlers registered for a writing method, and whether each reaches the
/// runtime only through `perform`.
fn writes(lib: &str) -> Vec<(String, bool)> {
    let mut handlers = BTreeSet::new();
    for line in lib.lines().filter(|l| l.contains(".route(")) {
        for method in ["post(", "put(", "delete(", "patch("] {
            for (k, _) in line.match_indices(method) {
                // A handler of another module (`welcome::open`): its name.
                let path: String = line[k + method.len()..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                    .collect();
                let name = path.rsplit("::").next().unwrap_or_default().to_owned();
                handlers.insert(name);
            }
        }
    }
    handlers
        .into_iter()
        .map(|h| {
            let body = lib
                .split(&format!("fn {h}("))
                .nth(1)
                .and_then(|b| b.split("\n}\n").next())
                .unwrap_or("");
            let perform = body.contains("perform(");
            (h, perform)
        })
        .collect()
}

#[test]
fn every_web_route_that_writes_goes_through_the_dispatcher() {
    let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("../speccoder-web/src");
    let jobs = std::fs::read_to_string(web.join("jobs.rs")).unwrap();
    assert!(
        !code_of(&jobs).contains("Runnable::"),
        "jobs.rs maps a kind itself: it only calls service::actions::perform"
    );
    assert!(code_of(&jobs).contains("perform("), "jobs.rs: no perform");
    // The routes are registered in lib.rs; a handler may live in a module
    // beside it (`welcome.rs`).
    let lib = std::fs::read_to_string(web.join("lib.rs")).unwrap()
        + &std::fs::read_to_string(web.join("welcome.rs")).unwrap_or_default();
    let errors: Vec<String> = writes(&lib)
        .into_iter()
        .filter(|(h, perform)| !perform && !WRITES_WITHOUT_A_KIND.iter().any(|(x, _)| x == h))
        .map(|(h, _)| {
            format!(
                "layering: the web route handler `{h}` writes without `service::actions::perform`: add a kind (the adding-a-kind contract of actions::perform)"
            )
        })
        .collect();
    assert!(errors.is_empty(), "\n{}", errors.join("\n"));
}

#[test]
fn the_route_scan_finds_a_planted_write() {
    let lib = "        .route(\"/api/x\", get(x).post(save_x))\n        .route(\"/api/y\", post(run_y))\nasync fn save_x() {\n    settings::apply();\n}\nasync fn run_y() {\n    service::actions::perform(a, b, d);\n}\n";
    assert_eq!(
        writes(lib),
        [("run_y".to_owned(), true), ("save_x".to_owned(), false)]
    );
}

/// Files that may name the dispatcher: itself, the facade, and the
/// interfaces a person acts through.
const DISPATCHERS: &[&str] = &[
    "speccoder-runtime/src/actions.rs",
    "speccoder-runtime/src/service.rs",
    "speccoder-web/src/jobs.rs",
    "speccoder/src/follow.rs",
    "speccoder/src/acp.rs",
    "speccoder/src/tui/mod.rs",
    // The line CLI's own commands (`config set` on a prefix list,
    // `approvals batch`).
    "speccoder/src/main.rs",
    // Plan through's one review question at a terminal: the approval is
    // the review kind, performed as the person.
    "speccoder/src/spec_through.rs",
];

/// Where code names the dispatcher outside [`DISPATCHERS`].
fn callers(files: &[(String, String)]) -> Vec<String> {
    let mut errors = Vec::new();
    for (rel, text) in files {
        if DISPATCHERS.contains(&rel.as_str()) {
            continue;
        }
        for (i, line) in code_of(text).lines().enumerate() {
            let named = [
                "actions::perform",
                "perform_for(",
                "perform_in(",
                "Dispatch {",
                "Dispatch::new(",
            ]
            .iter()
            .find(|n| line.contains(*n));
            if let Some(n) = named {
                errors.push(format!(
                    "layering: {rel}:{} names `{n}`: only an interface a person acts through takes an action",
                    i + 1
                ));
            }
        }
    }
    errors
}

#[test]
fn only_the_interfaces_a_person_acts_through_take_actions() {
    let files = sources();
    let errors = callers(&files);
    assert!(errors.is_empty(), "\n{}", errors.join("\n"));
    // The dashboard's `n` hands off to the line CLI's offer; it maps no
    // kind itself.
    let (_, tui) = files
        .iter()
        .find(|(r, _)| r == "speccoder/src/tui/mod.rs")
        .unwrap();
    assert!(code_of(tui).contains("perform_for(Interface::Dashboard"));
    let (_, main) = files
        .iter()
        .find(|(r, _)| r == "speccoder/src/main.rs")
        .unwrap();
    assert!(main.contains("tui::handed_off()") && main.contains("follow::offer("));
}

#[test]
fn the_caller_scan_finds_a_planted_one() {
    let tool = "fn approve_for_the_model(r: &Path) {\n    // actions::perform is not for tools\n    let _ = speccoder_runtime::actions::perform(r, None, d);\n}\n";
    let errors = callers(&[(
        "speccoder-runtime/src/tools/mod.rs".to_owned(),
        tool.to_owned(),
    )]);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("tools/mod.rs:3"), "{errors:?}");
}

/// Label, reason and command text the page may test because it decides
/// no action: what it displays, and what the person typed into a filter.
/// Each entry is a file and a piece of its line.
const UI_TEXT_ALLOWED: &[(&str, &str)] = &[
    // The input sheet shows the command with the person's words in it.
    (
        "components/InputSheet.svelte",
        "a.command.replace(\"\\\"…\\\"\"",
    ),
    // The palette filters by what the person typed.
    ("components/Palette.svelte", "a.label.includes(f)"),
    (
        "components/Palette.svelte",
        "a.label.toLowerCase().includes(f) || a.command.toLowerCase().includes(f) || a.why.toLowerCase().includes(f)",
    ),
    // A fix's button says the fix's first word.
    (
        "components/RemedyCard.svelte",
        "r.remedy.label.split(\" \")[0]",
    ),
    // Evidence rows and the inspector's sources are not actions.
    ("components/Drawer.svelte", "r.label === \"Model says\""),
    (
        "components/Drawer.svelte",
        "source?.label === \"Set Aside\"",
    ),
    (
        "components/Drawer.svelte",
        "source?.label === \"Last Attempt\"",
    ),
];

/// Where `text` (the UI source `file`) decides by an action's wording, or
/// writes an action itself outside `lib/actions.ts`.
fn ui_text_tests(file: &str, text: &str) -> Vec<(usize, String)> {
    const OPS: &[&str] = &[
        "===",
        "==",
        "!==",
        "!=",
        ".test(",
        ".includes(",
        ".startsWith(",
        ".endsWith(",
        ".match(",
        ".replace(",
        ".replaceAll(",
        ".split(",
    ];
    const CHAIN: &[&str] = &[".toLowerCase()", ".toUpperCase()", ".trim()", "?"];
    let mut found = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let code = line.trim_start();
        if code.starts_with("//") || code.starts_with('*') || code.starts_with("/*") {
            continue;
        }
        let allowed = |l: &str| {
            UI_TEXT_ALLOWED
                .iter()
                .any(|(f, piece)| *f == file && l.contains(piece))
        };
        for field in [".label", ".why", ".command"] {
            for (k, _) in line.match_indices(field) {
                let after = &line[k + field.len()..];
                if after.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let mut rest = after.trim_start();
                while let Some(c) = CHAIN.iter().find(|c| rest.starts_with(**c)) {
                    rest = &rest[c.len()..];
                }
                // What is tested comes after the object (`a`, `r.remedy`).
                let before = line[..k]
                    .trim_end_matches(|c: char| c.is_alphanumeric() || "_.?$".contains(c))
                    .trim_end();
                let tested = OPS.iter().any(|op| rest.starts_with(op))
                    || ["===", "==", "!==", "!="]
                        .iter()
                        .any(|op| before.ends_with(op) && !before.ends_with("=>"))
                    || before.ends_with(".test(");
                if tested && !allowed(line) {
                    found.push((i + 1, format!("tests `{field}`")));
                }
            }
        }
        if file != "lib/actions.ts" {
            for piece in ["perform({", ": Action = {", "Action => ({", " as Action"] {
                let written = line
                    .match_indices(piece)
                    .any(|(k, _)| !line[k + piece.len()..].starts_with('['));
                if written {
                    found.push((
                        i + 1,
                        format!("writes an action (`{piece}`): use offer() in lib/actions.ts"),
                    ));
                }
            }
        }
    }
    found
}

fn ui_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            ui_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "ts" || x == "svelte")
            && !p.ends_with("schema.gen.ts")
        {
            out.push(p);
        }
    }
}

#[test]
fn ui_label_text_never_decides_which_action_runs() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../speccoder-web/ui/src");
    let mut files = Vec::new();
    ui_files(&src, &mut files);
    files.sort();
    let mut errors = Vec::new();
    for f in &files {
        let rel = f.strip_prefix(&src).unwrap().display().to_string();
        let text = std::fs::read_to_string(f).unwrap();
        for (line, what) in ui_text_tests(&rel, &text) {
            errors.push(format!(
                "layering: ui/src/{rel}:{line} {what}: find an action by its id, kind or role (lib/actions.ts)"
            ));
        }
    }
    assert!(errors.is_empty(), "\n{}", errors.join("\n"));
}

#[test]
fn ui_label_scan_finds_a_planted_match() {
    let text = "const s = $derived(app.actions.find((a) => a.run?.kind === \"skill\" && /specify/.test(a.label)));\nconst x = actions.filter((a) => a.label.includes(t.id));\nconst y = a.why.toLowerCase().startsWith(\"x\");\nif (\"commit\" === a.command) go();\nvoid app.perform({ group: \"work\", run: { kind: \"fix_setup\" } });\n// a.label === \"x\" in a comment\nconst label = a.label;\n";
    let found: Vec<usize> = ui_text_tests("components/X.svelte", text)
        .into_iter()
        .map(|(l, _)| l)
        .collect();
    assert_eq!(found, [1, 2, 3, 4, 5]);
    // What the allow-list names passes.
    assert!(
        ui_text_tests(
            "components/Palette.svelte",
            "if (isMenu(a)) return f.length >= 2 && a.label.includes(f);"
        )
        .is_empty()
    );
}

#[test]
fn needs_you_has_a_place_for_every_kind_the_inbox_lists() {
    let page = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../speccoder-web/ui/src/components/Inbox.svelte"),
    )
    .unwrap();
    let sections = page
        .split("const kinds")
        .nth(1)
        .and_then(|s| s.split("\n  };").next())
        .unwrap();
    let missing: Vec<&str> = speccoder_runtime::inbox::KINDS
        .iter()
        .filter(|k| !sections.contains(&format!("\n    {k}: {{")))
        .copied()
        .collect();
    assert!(
        missing.is_empty(),
        "Inbox.svelte has no label for {missing:?} (they show under their raw name until it does)"
    );
    assert!(page.contains("kinds[i.kind] ??"), "no fallback label");
}

/// The allow lists an approval would widen: only the runtime's action arms
/// (`AllowAlways`, `WidenPolicy`) write them, never an interface by key.
const POLICY_KEYS: &[&str] = &["\"commands.allow\"", "\"sandbox.network_allow\""];

/// Interface files (the CLI, the web host, the page) naming an allow list's
/// key in code.
fn policy_writers(files: &[(String, String)]) -> Vec<String> {
    let mut errors = Vec::new();
    for (rel, text) in files {
        let interface = rel.starts_with("speccoder/src/")
            || rel.starts_with("speccoder-web/src/")
            || rel.starts_with("speccoder-web/ui/src/");
        if !interface || rel.ends_with("schema.gen.ts") {
            continue;
        }
        for (i, line) in code_of(text).lines().enumerate() {
            if let Some(k) = POLICY_KEYS.iter().find(|k| line.contains(*k)) {
                errors.push(format!(
                    "layering: {rel}:{} names {k}: the allow lists are written by the runtime's actions only (AllowAlways, WidenPolicy)",
                    i + 1
                ));
            }
        }
    }
    errors
}

#[test]
fn no_interface_writes_the_allow_lists_itself() {
    let mut files = sources();
    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("../speccoder-web/ui/src");
    let mut paths = Vec::new();
    ui_files(&ui, &mut paths);
    for p in paths {
        let rel = format!(
            "speccoder-web/ui/src/{}",
            p.strip_prefix(&ui).unwrap().display()
        );
        files.push((rel, std::fs::read_to_string(&p).unwrap()));
    }
    let errors = policy_writers(&files);
    assert!(errors.is_empty(), "\n{}", errors.join("\n"));
}

#[test]
fn the_policy_writer_scan_finds_a_planted_one() {
    let sheet =
        "await post(\"/api/config\", { changes: [{ key: \"commands.allow\", append: argv }] });\n";
    let errors = policy_writers(&[(
        "speccoder-web/ui/src/components/AskSheet.svelte".to_owned(),
        sheet.to_owned(),
    )]);
    assert_eq!(errors.len(), 1, "{errors:?}");
}
