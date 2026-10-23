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
const RUNTIME: &str = "speccoder-runtime";
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

    let names: BTreeMap<&str, &str> = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["id"].as_str().unwrap(), p["name"].as_str().unwrap()))
        .collect();

    let mut graph = Graph::new();
    for node in meta["resolve"]["nodes"].as_array().unwrap() {
        let from = names[node["id"].as_str().unwrap()].to_owned();
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
            .map(|d| names[d["pkg"].as_str().unwrap()].to_owned())
            .collect();
        graph
            .entry(from)
            .or_default()
            .extend::<BTreeSet<String>>(deps);
    }
    graph
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
        SPECKIT => &[CORE],
        RUNTIME => &[CORE, SPECKIT],
        BIN => &[CORE, SPECKIT, RUNTIME],
        _ => &[],
    }
}

fn violations(g: &Graph) -> Vec<String> {
    let workspace = [CORE, SPECKIT, RUNTIME, BIN];
    let mut errors = Vec::new();

    for (rule, krate) in [(1, CORE), (2, SPECKIT), (3, RUNTIME), (4, BIN)] {
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
        if CORE_DENY.contains(&dep.as_str()) {
            errors.push(format!(
                "layering: {CORE} must not depend on {dep} (rule 5: the core stays free of IO and async crates)"
            ));
        }
    }

    for (krate, deps) in g {
        if krate != RUNTIME && deps.contains("agentplane") {
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
    g.get_mut(CORE).unwrap().insert("tokio".to_owned());
    g.get_mut(BIN).unwrap().insert("agentplane".to_owned());
    let errors = violations(&g).join("\n");
    assert!(
        errors.contains("speccoder-speckit must not depend on speccoder-runtime (rule 2"),
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
