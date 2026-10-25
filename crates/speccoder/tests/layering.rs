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
