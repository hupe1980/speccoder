//! The format seam changes no byte a Spec Kit user or the model
//! sees: `inspect` of every supported Spec Kit version, the context bundle,
//! the acceptance input, the model-facing policies, the external-agent
//! prompt and the refusal texts are compared with captures made before the
//! refactor. `SPECCODER_BLESS=1` rewrites the captures (only ever before a
//! move — a difference after one is a bug in the move).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use speccoder_core::CommandData;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn captures() -> PathBuf {
    root().join("tests/fixtures/format-seam")
}

/// Compares `text` with capture `name`, or writes it when blessing.
fn golden(name: &str, text: &str) {
    let file = captures().join(name);
    if std::env::var_os("SPECCODER_BLESS").is_some() {
        fs::create_dir_all(captures()).unwrap();
        fs::write(&file, text).unwrap();
        return;
    }
    let want = fs::read_to_string(&file).unwrap_or_else(|_| {
        panic!("no capture {name}; run with SPECCODER_BLESS=1 before the move")
    });
    if want != text {
        let line = want
            .lines()
            .zip(text.lines())
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        panic!(
            "{name} differs from its capture at line {}:\n  was: {:?}\n  now: {:?}",
            line + 1,
            want.lines().nth(line),
            text.lines().nth(line)
        );
    }
}

/// Paths in captures are relative to the repository, so a capture does not
/// depend on where the checkout is.
fn portable(text: &str) -> String {
    let repo = root().join("../..").canonicalize().unwrap();
    text.replace(&repo.display().to_string(), "<repo>")
}

#[test]
fn inspect_of_every_supported_spec_kit_version_is_unchanged() {
    let compat = root().join("../speccoder-speckit/tests/fixtures/compat");
    let mut versions: Vec<_> = fs::read_dir(&compat)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    versions.sort();
    assert!(versions.len() >= 6);
    for v in versions {
        let result = speccoder_runtime::inspect(&compat.join(&v), None);
        let text = serde_json::to_string_pretty(&result).unwrap();
        golden(&format!("inspect-{v}.json"), &portable(&text));
    }
}

fn snapshot(project: &Path) -> speccoder_core::snapshot::ProjectSnapshot {
    match speccoder_runtime::inspect(project, None).data() {
        Some(CommandData::Inspect(s)) => (**s).clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn bundles_and_acceptance_input_are_unchanged() {
    let project = root().join("../../evals/calc-add/project");
    let snap = snapshot(&project);
    let bundle = speccoder_runtime::context::bundle(&snap, "T002").unwrap();
    let value = serde_json::to_value(&bundle).unwrap();
    golden(
        "bundle-T002.json",
        &serde_json::to_string_pretty(&value).unwrap(),
    );
    let input = speccoder_runtime::substrate::acceptance::input(
        value.clone(),
        &[],
        &["src/lib.rs".to_owned()],
        "Cargo.toml:\n[package]",
        &[],
    );
    golden(
        "acceptance-input.json",
        &serde_json::to_string_pretty(input.peek()).unwrap(),
    );
    golden(
        "external-prompt.txt",
        &speccoder_runtime::external::prompt(&value, None),
    );
    let digest = json!({ "attempt": 1, "reasons": ["checks failed"] });
    golden(
        "external-prompt-retry.txt",
        &speccoder_runtime::external::prompt(&value, Some(&digest)),
    );

    // A Convergence task gets the whole spec.
    let copy = tempfile::tempdir().unwrap();
    let ok = std::process::Command::new("cp")
        .arg("-R")
        .arg(project.join("."))
        .arg(copy.path())
        .status()
        .unwrap()
        .success();
    assert!(ok);
    let feature = snap.feature.as_ref().unwrap();
    let tasks = copy.path().join(&feature.tasks.as_ref().unwrap().path);
    let mut text = fs::read_to_string(&tasks).unwrap();
    text.push_str(
        "\n## Phase 9: Convergence\n\n- [ ] T099 Close the gap the converge step found\n",
    );
    fs::write(&tasks, text).unwrap();
    let converge = speccoder_runtime::context::bundle(&snapshot(copy.path()), "T099").unwrap();
    golden(
        "bundle-convergence.json",
        &serde_json::to_string_pretty(&converge).unwrap(),
    );
}

#[test]
fn model_facing_texts_are_unchanged() {
    use speccoder_runtime::format::speckit::WORDING;
    use speccoder_runtime::substrate::{acceptance, attempt, converge, host, monitor};
    golden("policy-attempt.txt", &attempt::policy(&WORDING));
    golden("policy-acceptance.txt", &acceptance::policy(&WORDING));
    golden("policy-converge.txt", &converge::policy(&WORDING));
    golden("policy-host.txt", &host::policy(&WORDING));
    golden("policy-monitor.txt", &monitor::policy(&WORDING));
}

#[test]
fn refusal_texts_are_unchanged() {
    use speccoder_runtime::tools::policy::{Decision, Policy};
    let intent = speccoder_runtime::tools::paths::Class::Intent.reason();
    let planning = Policy::new(&[], &[]).authoring();
    let refusal = match planning.classify(
        "write_file",
        &json!({ "path": "src/lib.rs", "content": "x" }),
    ) {
        Decision::Approval(r) | Decision::Deny(r) => r,
        Decision::Allow => panic!("allowed"),
    };
    let implement = match Policy::new(&[], &[]).classify(
        "write_file",
        &json!({ "path": "specs/001-x/spec.md", "content": "x" }),
    ) {
        Decision::Deny(r) => r,
        other => panic!("{other:?}"),
    };
    golden(
        "refusals.txt",
        &format!("{intent}\n{refusal}\n{implement}\n"),
    );
}

/// A copy of compat fixture 1.2.3, plus whatever `extra` adds.
fn project(extra: impl Fn(&Path)) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let ok = std::process::Command::new("cp")
        .arg("-R")
        .arg(root().join("../speccoder-speckit/tests/fixtures/compat/1.2.3/."))
        .arg(dir.path())
        .status()
        .unwrap()
        .success();
    assert!(ok);
    extra(dir.path());
    dir
}

fn bare(extra: impl Fn(&Path)) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    extra(dir.path());
    dir
}

fn openspec(p: &Path) {
    fs::create_dir_all(p.join("openspec/changes/add-x")).unwrap();
    fs::write(p.join("openspec/changes/add-x/tasks.md"), "- [ ] 1.1 x\n").unwrap();
}

fn kiro(p: &Path) {
    fs::create_dir_all(p.join(".kiro/specs/x")).unwrap();
    fs::write(p.join(".kiro/specs/x/requirements.md"), "# R\n").unwrap();
    fs::write(p.join(".kiro/specs/x/tasks.md"), "- [ ] 1. x\n").unwrap();
}

fn files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p.display().to_string());
            }
        }
    }
    out.sort();
    out
}

#[test]
fn formats_are_detected_chosen_or_refused() {
    use speccoder_core::CommandStatus;
    use speccoder_core::spec::FormatKind;
    use speccoder_runtime::format::open_with;

    let started = std::time::Instant::now();
    let speckit = project(|_| {});
    let both = project(openspec);
    let only_openspec = bare(openspec);
    let only_kiro = bare(kiro);
    let nothing = bare(|_| {});
    let before: Vec<_> = [&speckit, &both, &only_openspec, &only_kiro, &nothing]
        .iter()
        .map(|d| files(d.path()))
        .collect();

    // Spec Kit detected or chosen: as before.
    for choice in [None, Some(FormatKind::Speckit)] {
        let o = open_with(speckit.path(), None, choice).ok().unwrap();
        assert_eq!(o.adapter.kind(), FormatKind::Speckit);
        assert!(
            o.snapshot
                .diagnostics
                .iter()
                .all(|d| !d.code.starts_with("format."))
        );
    }
    // Two formats this version runs: the person chooses.
    let r = open_with(both.path(), None, None).err().unwrap();
    assert_eq!(r.status, CommandStatus::DiagnosticsError);
    assert!(r.message.contains("speckit, openspec"), "{}", r.message);
    assert!(
        r.message.contains("choose one with --format"),
        "{}",
        r.message
    );
    for kind in [FormatKind::Speckit, FormatKind::Openspec] {
        let o = open_with(both.path(), None, Some(kind)).ok().unwrap();
        assert_eq!(o.adapter.kind(), kind);
    }
    // OpenSpec detected alone.
    let o = open_with(only_openspec.path(), None, None).ok().unwrap();
    assert_eq!(o.adapter.kind(), FormatKind::Openspec);
    assert_eq!(o.snapshot.feature.unwrap().feature.name, "add-x");
    // A format chosen where there is none: that format's own answer.
    let r = open_with(speckit.path(), None, Some(FormatKind::Openspec))
        .err()
        .unwrap();
    assert_eq!(r.status, CommandStatus::NotAProject);
    assert!(
        r.message
            .starts_with("no OpenSpec project (openspec/) at or above "),
        "{}",
        r.message
    );
    // A format this version does not run.
    let r = open_with(speckit.path(), None, Some(FormatKind::Kiro))
        .err()
        .unwrap();
    assert_eq!(
        (r.status, r.message.as_str()),
        (
            CommandStatus::NotAProject,
            "speccoder runs Spec Kit and OpenSpec projects, not Kiro specs"
        )
    );
    let r = open_with(only_kiro.path(), None, None).err().unwrap();
    assert_eq!(r.status, CommandStatus::NotAProject);
    assert!(
        r.message.starts_with("found a Kiro project at "),
        "{}",
        r.message
    );
    assert!(r.message.ends_with("not Kiro specs"));
    // Kiro beside Spec Kit: Spec Kit, with a warning.
    let with_kiro = project(kiro);
    let o = open_with(with_kiro.path(), None, None).ok().unwrap();
    assert_eq!(o.adapter.kind(), FormatKind::Speckit);
    let w: Vec<_> = o
        .snapshot
        .diagnostics
        .iter()
        .filter(|d| d.code == "format.unsupported-marker")
        .collect();
    assert_eq!(w.len(), 1, "{:?}", o.snapshot.diagnostics);
    assert!(w[0].message.ends_with("not Kiro specs"));
    // Nothing: today's message.
    let r = speccoder_runtime::inspect(nothing.path(), None);
    assert_eq!(r.status(), CommandStatus::NotAProject);
    assert!(
        r.message()
            .starts_with("no Spec Kit project (.specify/) at or above ")
    );

    // `[spec] format` decides when no flag does; a flag beats it.
    fs::write(
        speckit.path().join("speccoder.toml"),
        "[spec]\nformat = \"kiro\"\n",
    )
    .unwrap();
    let r = open_with(speckit.path(), None, None).err().unwrap();
    assert!(r.message.ends_with("not Kiro specs"));
    assert!(open_with(speckit.path(), None, Some(FormatKind::Speckit)).is_ok());
    fs::write(
        speckit.path().join("speccoder.toml"),
        "[spec]\nformat = \"bogus\"\n",
    )
    .unwrap();
    let r = speccoder_runtime::inspect(speckit.path(), None);
    assert_eq!(r.status(), CommandStatus::ConfigError);
    assert_eq!(
        r.message(),
        "speccoder.toml: [spec]: format = \"bogus\" (speckit, openspec or kiro)"
    );
    fs::remove_file(speckit.path().join("speccoder.toml")).unwrap();

    let after: Vec<_> = [&speckit, &both, &only_openspec, &only_kiro, &nothing]
        .iter()
        .map(|d| files(d.path()))
        .collect();
    assert_eq!(before, after, "nothing is written");
    // Detection waits on no network or model: a hang shows here. Speed is
    // the cold-start gate's (a second-scale bound flaked under a loaded,
    // parallel suite: 1.4 s on Linux in Docker).
    assert!(started.elapsed() < std::time::Duration::from_secs(15));
}
