//! Bugs in Spec Kit's layout. With the first-party `bug` extension
//! installed and enabled, a bug is the extension's own
//! `.specify/bugs/<slug>/assessment.md`, written by its assess skill —
//! speccoder writes nothing there and only reads the person's words back.
//! Without it, the standing unit `NNN-bugfixes`: each bug a user story
//! whose acceptance scenarios are the triad, an approved requirement in
//! triad form and a task citing it, appended.
//!
//! The parse of `assessment.md` is pure (no model, no tool), so a verifier
//! reads a bug's triad from a commit's tree.

use std::path::Path;

use speccoder_core::bug::{BugHome, HomeKind, Triad, normalise, valid_slug};

use super::quick::highest;
use crate::format::{BugRefusal, BugStep, ReadBug, RecordedBug, Rules};

/// The standing unit's name, after its number.
const NAME: &str = "bugfixes";
/// Where the extension keeps bugs.
const BUGS: &str = ".specify/bugs";
/// The extension's id.
const EXTENSION: &str = "bug";
/// Its assessment file.
const ASSESSMENT: &str = "assessment.md";

/// What the extension's assess skill may write: only bug directories (a
/// second slug is caught after the run).
pub static ASSESS_RULES: Rules = Rules {
    intent: super::texts::RULES.intent,
    intent_reason: super::texts::RULES.intent_reason,
    authoring_path: |p| p.starts_with(".specify/bugs/") && !p.contains(".."),
    command: |_, _| None,
    env: &[],
    authoring_refusal: |p| {
        format!(
            "the `bug` extension's assess step writes only its bug's directory (.specify/bugs/<slug>/); `{p}` is not in it"
        )
    },
};

/// The format's own bug step: the extension's assess skill, given the
/// person's labelled triad verbatim and the slug.
pub(super) const STEP: BugStep = BugStep {
    skill: "bug-assess",
    arguments: |triad, slug| format!("{}slug={slug}", triad.labelled()),
    rules: &ASSESS_RULES,
};

/// Where a bug lives here, with why.
pub(super) fn unit(root: &Path) -> Result<BugHome, BugRefusal> {
    let inventory = super::extensions::inventory(root);
    let listed = inventory
        .as_ref()
        .and_then(|i| i.entries.iter().find(|e| e.id == EXTENSION));
    let from = |i: &crate::format::Inventory| match i.source {
        crate::format::InventorySource::Cli => "`specify extension list`",
        crate::format::InventorySource::Files => ".specify/extensions/.registry",
    };
    if let (Some(e), Some(i)) = (listed, inventory.as_ref())
        && e.enabled
    {
        return Ok(BugHome {
            kind: HomeKind::NativeSpeckitBug,
            location: BUGS.to_owned(),
            reason: format!(
                "Spec Kit's `bug` extension {} is installed and enabled ({}): the bug is the extension's own",
                e.version,
                from(i)
            ),
        });
    }
    let reason = match (listed, inventory.as_ref()) {
        (Some(e), Some(i)) => format!(
            "Spec Kit's `bug` extension {} is installed but disabled ({}): the standing bug-fix unit",
            e.version,
            from(i)
        ),
        _ => "Spec Kit without the `bug` extension: the standing bug-fix unit".to_owned(),
    };
    Ok(BugHome {
        kind: HomeKind::StandingSpeckit,
        location: format!(
            "{}/{}",
            speccoder_speckit::features::SPECS,
            standing_name(root)
        ),
        reason,
    })
}

/// The standing unit's directory name: the existing one, else the next number.
fn standing_name(root: &Path) -> String {
    let specs = root.join(speccoder_speckit::features::SPECS);
    let names: Vec<String> = std::fs::read_dir(&specs)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    if let Some(n) = names.iter().find(|n| {
        n.ends_with(&format!("-{NAME}")) && speccoder_speckit::features::prefix(n).is_some()
    }) {
        return n.clone();
    }
    let next = names
        .iter()
        .filter_map(|n| {
            let p = speccoder_speckit::features::prefix(n)?.to_owned();
            (p.len() <= 4).then(|| p.parse::<u32>().ok()).flatten()
        })
        .max()
        .unwrap_or(0)
        + 1;
    format!("{next:03}-{NAME}")
}

/// The extension's directory and unit name for `slug` (`None`: not this
/// layout, or not a slug — never a path).
pub(super) fn dir(home: &BugHome, slug: &str) -> Option<(String, String)> {
    (home.kind == HomeKind::NativeSpeckitBug && valid_slug(slug))
        .then(|| (format!("{BUGS}/{slug}"), format!("bugs/{slug}")))
}

/// Whether `path` (or its directory chain under `root`) is a symbolic link:
/// a bug directory that is one could point anywhere.
fn linked(root: &Path, rel: &[&str]) -> bool {
    let mut p = root.to_path_buf();
    rel.iter().any(|part| {
        p.push(part);
        std::fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_symlink())
    })
}

/// A slug for `title`: two to four kebab-case words, made unique.
pub(super) fn slug(root: &Path, title: &str) -> String {
    let words: Vec<String> = title
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .take(4)
        .collect();
    let base = if words.is_empty() {
        "bug".to_owned()
    } else {
        words.join("-")
    };
    let taken = |s: &str| root.join(BUGS).join(s).exists();
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|s| !taken(s))
        .unwrap_or(base)
}

/// Records `triad` in the standing unit, creating it the first time.
pub(super) fn record(
    root: &Path,
    home: &BugHome,
    triad: &Triad,
) -> Result<RecordedBug, BugRefusal> {
    if home.kind != HomeKind::StandingSpeckit {
        return Err(BugRefusal::new(
            "BUG-NO-UNIT",
            "the bug lives in the `bug` extension's own layout: its assess step records it",
        ));
    }
    let io = |e: String| BugRefusal::new("INTERNAL", e);
    let name = home
        .location
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_owned();
    let dir = root.join(&home.location);
    std::fs::create_dir_all(&dir).map_err(|e| io(format!("{}: {e}", dir.display())))?;
    let spec_path = dir.join("spec.md");
    let tasks_path = dir.join("tasks.md");
    let mut spec = std::fs::read_to_string(&spec_path).unwrap_or_else(|_| {
        format!(
            "# Feature Specification: Bug Fixes\n\n**Feature Branch**: `{name}`\n**Status**: Ongoing\n**Input**: Bugs reported one at a time (`speccoder bug`). Each is a user story whose acceptance scenarios are the reporter's words — Current, Expected, Unchanged — with its own requirement and task, reproduced before it is fixed.\n\n## User Scenarios & Testing\n"
        )
    });
    let mut tasks =
        std::fs::read_to_string(&tasks_path).unwrap_or_else(|_| "# Tasks: Bug Fixes\n".to_owned());
    let story = highest(&spec, "### User Story ") + 1;
    let fr = highest(&spec, "**FR-") + 1;
    let task = highest(&tasks, "- [ ] T")
        .max(highest(&tasks, "- [x] T"))
        .max(highest(&tasks, "- [X] T"))
        + 1;
    let t = triad.title();
    let requirement = format!("FR-{fr:03}");
    let mut block = format!(
        "\n### User Story {story} - {t} (Priority: P1)\n\nA bug, in the reporter's words.\n\n**Acceptance Scenarios**:\n\n1. **Current**: {}\n",
        triad.current.text
    );
    for (n, s) in (2..).zip(triad.expected.iter().chain(&triad.unchanged)) {
        let label = if s.id.starts_with('E') {
            "Expected"
        } else {
            "Unchanged"
        };
        block.push_str(&format!("{n}. **{label}**: {}\n", s.text));
    }
    let mut line = format!("Current: {}.", triad.current.text.trim_end_matches('.'));
    for e in &triad.expected {
        line.push_str(&format!(" Expected: {}.", e.text.trim_end_matches('.')));
    }
    for u in &triad.unchanged {
        line.push_str(&format!(" Unchanged: {}.", u.text.trim_end_matches('.')));
    }
    block.push_str(&format!("\n- **{requirement}**: {line}\n"));
    if !spec.ends_with('\n') {
        spec.push('\n');
    }
    spec.push_str(&block);
    let task_id = format!("T{task:03}");
    if !tasks.ends_with('\n') {
        tasks.push('\n');
    }
    tasks.push_str(&format!(
        "\n## Phase {story}: User Story {story} - {t} (Priority: P1)\n\n- [ ] {task_id} [US{story}] Fix: {t} per {requirement}\n"
    ));
    // The person's words read back from what is about to be written,
    // exactly (no markup of theirs stripped), or nothing is written.
    let back = standing_triad(&spec, &requirement).map_err(|_| unfaithful(&requirement))?;
    if !same_words(&back, triad) {
        return Err(unfaithful(&requirement));
    }
    // Appends only: what was there is an exact prefix of what is written,
    // checked again right before writing (principle I).
    for (path, new) in [(&spec_path, &spec), (&tasks_path, &tasks)] {
        if let Ok(now) = std::fs::read_to_string(path)
            && !new.starts_with(now.trim_end_matches('\n'))
        {
            return Err(BugRefusal::new(
                "INTERNAL",
                format!(
                    "{} changed while the bug was being recorded; try again",
                    path.display()
                ),
            ));
        }
    }
    std::fs::write(&spec_path, spec).map_err(|e| io(format!("{}: {e}", spec_path.display())))?;
    std::fs::write(&tasks_path, tasks).map_err(|e| io(format!("{}: {e}", tasks_path.display())))?;
    Ok(RecordedBug {
        unit: name,
        dir: home.location.clone(),
        requirement,
        task: task_id,
        title: t,
        files: vec![
            format!("{}/spec.md", home.location),
            format!("{}/tasks.md", home.location),
        ],
    })
}

/// The refusal of a triad whose words would not read back as given.
pub(crate) fn unfaithful(requirement: &str) -> BugRefusal {
    BugRefusal::new(
        "BUG-ASSESSMENT-DRIFT",
        format!(
            "{requirement} would not read back exactly your words (a statement holds what the file's own markup means); nothing is written — reword it"
        ),
    )
}

/// `read` carries exactly `triad`'s statements, in order.
pub(crate) fn same_words(read: &ReadBug, triad: &Triad) -> bool {
    read.current == [triad.current.text.clone()]
        && read.expected
            == triad
                .expected
                .iter()
                .map(|s| s.text.clone())
                .collect::<Vec<_>>()
        && read.unchanged
            == triad
                .unchanged
                .iter()
                .map(|s| s.text.clone())
                .collect::<Vec<_>>()
}

/// The triad as the home holds it: a native bug by slug, a standing one by
/// requirement.
pub(super) fn read(root: &Path, home: &BugHome, key: &str) -> Result<ReadBug, BugRefusal> {
    match home.kind {
        HomeKind::NativeSpeckitBug => {
            if !valid_slug(key) {
                return Err(BugRefusal::new(
                    "BUG-ASSESSMENT-UNREADABLE",
                    format!("`{key}` is not a bug slug"),
                ));
            }
            if linked(root, &[".specify", "bugs", key, ASSESSMENT]) {
                return Err(BugRefusal::new(
                    "BUG-ASSESSMENT-UNREADABLE",
                    format!("{BUGS}/{key} is reached through a symbolic link: it is not read"),
                ));
            }
            let file = root.join(BUGS).join(key).join(ASSESSMENT);
            let text = std::fs::read_to_string(&file).map_err(|_| {
                BugRefusal::new(
                    "BUG-ASSESSMENT-UNREADABLE",
                    format!("{BUGS}/{key}/{ASSESSMENT} is missing"),
                )
            })?;
            parse_assessment(&text).map(|mut r| {
                r.file = format!("{BUGS}/{key}/{ASSESSMENT}");
                r
            })
        }
        _ => {
            let text = std::fs::read_to_string(root.join(&home.location).join("spec.md")).map_err(
                |_| {
                    BugRefusal::new(
                        "BUG-NO-UNIT",
                        format!("{}/spec.md is missing", home.location),
                    )
                },
            )?;
            standing_triad(&text, key).map(|mut r| {
                r.file = format!("{}/spec.md", home.location);
                r
            })
        }
    }
}

/// The labelled statement a line holds: `Current: …`, `**Expected**: …`,
/// `> Unchanged: …`, `1. **Current**: …`. The list or quote marker and the
/// label are matched exactly at the start; the words after the label are
/// the person's, kept byte for byte (whitespace normalised) — markup in
/// them is theirs, never stripped.
fn labelled(line: &str) -> Option<(&'static str, String)> {
    let mut l = line.trim_start();
    loop {
        let before = l;
        if let Some(r) = l.strip_prefix('>') {
            l = r.trim_start();
        } else if let Some(r) = ["- ", "* ", "+ "].iter().find_map(|m| l.strip_prefix(m)) {
            l = r.trim_start();
        } else if let Some((n, rest)) = l.split_once(". ")
            && !n.is_empty()
            && n.chars().all(|c| c.is_ascii_digit())
        {
            l = rest.trim_start();
        }
        if l == before {
            break;
        }
    }
    for label in ["current", "expected", "unchanged"] {
        for (open, close) in [("**", "**:"), ("**", ":**"), ("", ":")] {
            let Some(rest) = l.strip_prefix(open) else {
                continue;
            };
            let Some(word) = rest.get(..label.len()) else {
                continue;
            };
            if !word.eq_ignore_ascii_case(label) {
                continue;
            }
            if let Some(text) = rest[label.len()..].strip_prefix(close) {
                let text = normalise(text);
                return (!text.is_empty()).then_some((label, text));
            }
        }
    }
    None
}

fn collect(lines: impl Iterator<Item = String>) -> ReadBug {
    let mut r = ReadBug {
        current: Vec::new(),
        expected: Vec::new(),
        unchanged: Vec::new(),
        title: None,
        file: String::new(),
    };
    for line in lines {
        if let Some((label, text)) = labelled(&line) {
            match label {
                "current" => r.current.push(text),
                "expected" => r.expected.push(text),
                _ => r.unchanged.push(text),
            }
        }
    }
    r
}

/// `assessment.md` in the extension's layout (1.0.1): a `# Bug Assessment:`
/// title, a `## Report` and a `## Symptom` section, and the triad as
/// labelled lines. Any other shape is refused, never guessed at.
///
/// # Errors
///
/// `BUG-ASSESSMENT-UNREADABLE`: not that layout, or no Current and
/// Expected lines.
pub fn parse_assessment(text: &str) -> Result<ReadBug, BugRefusal> {
    let unreadable = |why: &str| {
        BugRefusal::new(
            "BUG-ASSESSMENT-UNREADABLE",
            format!("{ASSESSMENT} is not in the `bug` extension's layout speccoder reads: {why}"),
        )
    };
    let title = text
        .lines()
        .find_map(|l| l.strip_prefix("# Bug Assessment:"))
        .map(|t| t.trim().to_owned())
        .ok_or_else(|| unreadable("no `# Bug Assessment:` title"))?;
    let heading = |h: &str| text.lines().any(|l| l.trim_start().starts_with(h));
    if !heading("## Report") || !heading("## Symptom") {
        return Err(unreadable("no `## Report` and `## Symptom` sections"));
    }
    // The person's words are in the Report section (quoted or listed).
    let report: Vec<String> = text
        .lines()
        .skip_while(|l| !l.trim_start().starts_with("## Report"))
        .skip(1)
        .take_while(|l| !l.trim_start().starts_with("## "))
        .map(str::to_owned)
        .collect();
    let mut r = collect(report.into_iter());
    r.title = Some(title);
    if r.current.is_empty() || r.expected.is_empty() {
        return Err(unreadable(
            "its Report section has no `Current:` and `Expected:` lines",
        ));
    }
    Ok(r)
}

/// A standing unit's story for `requirement`: its labelled scenarios.
fn standing_triad(spec: &str, requirement: &str) -> Result<ReadBug, BugRefusal> {
    let mut story: Vec<&str> = Vec::new();
    let mut found = None;
    for line in spec.lines() {
        if line.starts_with("### User Story ") {
            if found.is_some() {
                break;
            }
            story.clear();
        }
        story.push(line);
        if line.contains(&format!("**{requirement}**")) {
            found = Some(story.clone());
        }
    }
    let lines = found.ok_or_else(|| {
        BugRefusal::new(
            "BUG-NO-UNIT",
            format!("no requirement {requirement} in the bug-fix unit"),
        )
    })?;
    // The scenarios only: the requirement line repeats them in a sentence.
    let r = collect(
        lines
            .iter()
            .filter(|l| !l.contains(&format!("**{requirement}**")))
            .map(|l| (*l).to_owned()),
    );
    if r.current.is_empty() || r.expected.is_empty() {
        return Err(BugRefusal::new(
            "BUG-ASSESSMENT-UNREADABLE",
            format!("{requirement}'s story has no Current and Expected scenarios"),
        ));
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!(
        "../../../../speccoder-speckit/tests/fixtures/compat/1.1.3-bug-ext/.specify/bugs/empty-title-crash/assessment.md"
    );

    #[test]
    fn the_fixture_assessment_reads_back_the_triad() {
        let r = parse_assessment(FIXTURE).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(r.current, ["saving an empty title crashes"]);
        assert_eq!(r.expected, ["an empty title is refused with a message"]);
        assert_eq!(r.unchanged, ["a non-empty title is saved as typed"]);
        assert_eq!(r.title.as_deref(), Some("Saving an empty title crashes"));
    }

    #[test]
    fn another_shape_is_refused_not_guessed() {
        let e = parse_assessment("# Notes\n\nCurrent: x\nExpected: y\n").err();
        assert_eq!(e.map(|e| e.code), Some("BUG-ASSESSMENT-UNREADABLE"));
        let no_lines = FIXTURE.replace("> Expected:", "> Hoped:");
        assert_eq!(
            parse_assessment(&no_lines).err().map(|e| e.code),
            Some("BUG-ASSESSMENT-UNREADABLE")
        );
    }

    #[test]
    fn labels_are_read_in_every_list_form() {
        for l in [
            "Current: a  b",
            "> Current: a b",
            "- **Current**: a b",
            "1. **Current**: a b",
            "  * current: a b",
        ] {
            assert_eq!(labelled(l), Some(("current", "a b".to_owned())), "{l}");
        }
        assert_eq!(labelled("Severity: high"), None);
        // The person's own markup stays theirs.
        assert_eq!(
            labelled("2. **Expected**: passing **kwargs drops them"),
            Some(("expected", "passing **kwargs drops them".to_owned()))
        );
        assert_eq!(
            labelled("> Expected: `*args` and **kwargs**: kept"),
            Some(("expected", "`*args` and **kwargs**: kept".to_owned()))
        );
        assert_eq!(
            labelled("**Current:** x"),
            Some(("current", "x".to_owned()))
        );
    }

    proptest::proptest! {
        #[test]
        fn the_standing_unit_reads_back_exactly_what_it_records(
            current in "[ -~]{1,40}",
            expected in proptest::collection::vec("[ -~]{1,40}", 1..3),
            unchanged in proptest::collection::vec("[ -~]{1,40}", 0..2),
        ) {
            let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
            let Ok(triad) = Triad::new(&current, &expected, &unchanged, crate::integrity::sha256_hex) else {
                return Ok(());
            };
            let home = BugHome {
                kind: HomeKind::StandingSpeckit,
                location: "specs/001-bugfixes".to_owned(),
                reason: String::new(),
            };
            // Recorded words read back exactly, or nothing is written.
            match record(dir.path(), &home, &triad) {
                Ok(rec) => {
                    let back = read(dir.path(), &home, &rec.requirement).unwrap_or_else(|e| panic!("{e:?}"));
                    proptest::prop_assert!(same_words(&back, &triad), "{back:?} vs {triad:?}");
                }
                Err(e) => {
                    proptest::prop_assert_eq!(e.code, "BUG-ASSESSMENT-DRIFT");
                    proptest::prop_assert!(!dir.path().join("specs/001-bugfixes/spec.md").exists());
                }
            }
        }
    }

    #[test]
    #[cfg(unix)]
    fn a_slug_never_reaches_outside_the_bugs_directory() {
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let home = BugHome {
            kind: HomeKind::NativeSpeckitBug,
            location: BUGS.to_owned(),
            reason: String::new(),
        };
        std::fs::create_dir_all(tmp.path().join("elsewhere")).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(tmp.path().join("elsewhere/assessment.md"), FIXTURE)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(dir(&home, "../../elsewhere"), None);
        assert_eq!(
            read(tmp.path(), &home, "../../elsewhere")
                .err()
                .map(|e| e.code),
            Some("BUG-ASSESSMENT-UNREADABLE")
        );
        // A bug directory that is a symbolic link is not read.
        std::fs::create_dir_all(tmp.path().join(BUGS)).unwrap_or_else(|e| panic!("{e}"));
        std::os::unix::fs::symlink(
            tmp.path().join("elsewhere"),
            tmp.path().join(BUGS).join("linked"),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            read(tmp.path(), &home, "linked").err().map(|e| e.code),
            Some("BUG-ASSESSMENT-UNREADABLE")
        );
    }

    #[test]
    fn slugs_are_kebab_case_and_unique() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            slug(dir.path(), "Saving an empty title crashes"),
            "saving-an-empty-title"
        );
        std::fs::create_dir_all(dir.path().join(".specify/bugs/saving-an-empty-title"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            slug(dir.path(), "Saving an empty title crashes"),
            "saving-an-empty-title-2"
        );
    }
}

#[cfg(test)]
mod policy_tests {
    #[test]
    fn the_assess_step_writes_its_bug_directory_only() {
        let p = crate::tools::policy::Policy::new(&[], &[])
            .rules(&super::ASSESS_RULES)
            .authoring();
        let d = |path: &str| {
            p.classify(
                "write_file",
                &serde_json::json!({ "path": path, "content": "x" }),
                None,
            )
        };
        assert_eq!(
            d(".specify/bugs/x/assessment.md"),
            crate::tools::policy::Decision::Allow
        );
        assert_ne!(d("src/lib.rs"), crate::tools::policy::Decision::Allow);
        assert_ne!(
            d("specs/001-x/spec.md"),
            crate::tools::policy::Decision::Allow
        );
    }
}
