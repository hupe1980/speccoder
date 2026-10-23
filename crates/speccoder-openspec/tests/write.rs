//! The engine's tick and convergence append on an OpenSpec `tasks.md`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

use speccoder_openspec::{sha256_hex, tasks, write};

const SAMPLE: &str =
    "tests/fixtures/1.14.3/sample/openspec/changes/add-farewell-and-locale/tasks.md";

fn copy(text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("tasks.md");
    fs::write(&p, text).unwrap();
    (dir, p)
}

fn hash_of(text: &str, id: &str) -> String {
    let (parsed, _) = tasks::parse("tasks.md", text);
    parsed
        .tasks
        .iter()
        .find(|t| t.id == id)
        .unwrap()
        .line_hash
        .clone()
}

#[test]
fn a_tick_changes_exactly_one_byte() {
    let text = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(SAMPLE)).unwrap();
    let (_d, p) = copy(&text);
    write::tick(&p, "1.2", &hash_of(&text, "1.2")).unwrap();
    let after = fs::read_to_string(&p).unwrap();
    let diff: Vec<_> = text
        .bytes()
        .zip(after.bytes())
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(diff, [(b' ', b'x')]);
    assert_eq!(text.len(), after.len());
    assert!(
        write::tick(&p, "1.2", &hash_of(&text, "1.2"))
            .unwrap_err()
            .contains("already ticked")
    );
    assert!(
        write::tick(&p, "2.1", "stale")
            .unwrap_err()
            .contains("changed")
    );
}

#[test]
fn other_markers_and_open_children_are_not_ticked() {
    let text = "## 1. A\n\n- [~] 1.1 a\n- [] 1.2 b\n- [ ] 1.3 parent\n  - [ ] 1.3.1 child\n";
    let (_d, p) = copy(text);
    assert!(
        write::tick(&p, "1.1", &hash_of(text, "1.1"))
            .unwrap_err()
            .contains("`[~]`")
    );
    assert!(
        write::tick(&p, "1.2", &hash_of(text, "1.2"))
            .unwrap_err()
            .contains("`[]`")
    );
    assert!(
        write::tick(&p, "1.3", &hash_of(text, "1.3"))
            .unwrap_err()
            .contains("open nested task")
    );
    write::tick(&p, "1.3.1", &hash_of(text, "1.3.1")).unwrap();
    write::tick(&p, "1.3", &hash_of(text, "1.3")).unwrap();
    assert_eq!(
        fs::read_to_string(&p).unwrap(),
        text.replace("[ ] 1.3", "[x] 1.3")
    );
}

#[test]
fn convergence_is_the_next_numbered_group() {
    let text = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(SAMPLE)).unwrap();
    let (_d, p) = copy(&text);
    let ids = write::append_convergence(
        &p,
        &sha256_hex(text.as_bytes()),
        &[
            "Handle `--bye` without a name".to_owned(),
            "- [ ] Second".to_owned(),
        ],
    )
    .unwrap();
    assert_eq!(ids, ["3.1", "3.2"]);
    let after = fs::read_to_string(&p).unwrap();
    assert!(
        after.starts_with(&text),
        "the old content is an exact prefix"
    );
    assert!(after.ends_with(
        "\n## 3. Convergence\n\n- [ ] 3.1 Handle `--bye` without a name\n- [ ] 3.2 Second\n"
    ));
    let (parsed, _) = tasks::parse("tasks.md", &after);
    assert_eq!(
        parsed.tasks.last().unwrap().phase.as_deref(),
        Some("3. Convergence")
    );
    assert!(
        write::append_convergence(&p, &sha256_hex(text.as_bytes()), &["x".to_owned()])
            .unwrap_err()
            .contains("changed since convergence began")
    );
}
