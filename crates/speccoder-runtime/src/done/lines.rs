//! The definition of done's lines, each a pure function of what
//! [`super::Facts`] read: no git, no disk, so each rule is tested alone.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;
use speccoder_core::done::{DoneFinding, DoneLine, DonePart, DoneState};

/// A docs page as read.
#[derive(Debug, Clone)]
pub struct Page {
    /// Repository-relative path.
    pub path: String,
    /// Its text.
    pub text: String,
}

/// A declared screenshot, as history has it.
#[derive(Debug, Clone, Default)]
pub struct Shot {
    /// The declaration's name (`[screenshots.<name>]`).
    pub name: String,
    /// Its `shot` glob.
    pub glob: String,
    /// The shot files it matched.
    pub files: Vec<String>,
    /// When the shots were last committed (Unix seconds); none: never.
    pub shot_time: Option<i64>,
    /// Whether a shot file differs from its commit.
    pub shot_dirty: bool,
    /// The newest committed source: its path and time.
    pub newest_source: Option<(String, i64)>,
    /// Sources that differ from their commit.
    pub dirty_sources: Vec<String>,
}

fn finding(path: &str, line: Option<usize>, what: impl Into<String>) -> DoneFinding {
    DoneFinding {
        path: path.to_owned(),
        line: line.and_then(|l| u32::try_from(l).ok()),
        what: what.into(),
    }
}

/// Whether a changed path is beside the product: tests, docs, CI, the
/// changelog, dot-directories. Anything else is user-facing.
#[must_use]
pub fn is_aside(path: &str, docs: &BTreeSet<String>) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    docs.contains(path)
        || lower.ends_with(".md")
        || lower.ends_with(".mdx")
        || name.starts_with("changelog")
        || name.contains("_test.")
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.starts_with("test_")
        || lower.split('/').any(|c| {
            c.starts_with('.')
                || matches!(c, "tests" | "test" | "fixtures" | "benches" | "__tests__")
        })
}

/// The `## [Unreleased]` section's list items; `None`: no such section.
#[must_use]
pub fn unreleased_entries(changelog: &str) -> Option<usize> {
    section_entries(changelog, |l| {
        l.starts_with("## [unreleased]") || l.trim_end() == "## unreleased"
    })
}

/// The list items of the first section whose lowercased heading matches.
fn section_entries(changelog: &str, heading: impl Fn(&str) -> bool) -> Option<usize> {
    let mut lines = changelog.lines();
    lines.find(|l| heading(&l.trim_start().to_ascii_lowercase()))?;
    Some(
        lines
            .take_while(|l| !l.starts_with("## "))
            .filter(|l| {
                let t = l.trim_start();
                t.starts_with("- ") || t.starts_with("* ")
            })
            .count(),
    )
}

/// The changelog line: `None` without a changelog. `cut`: the project's
/// version when no release tag has it yet — a release being made, whose
/// entries moved from `[Unreleased]` into its own section.
#[must_use]
pub fn changelog(
    file: &str,
    text: Option<&str>,
    changed: &[String],
    docs: &BTreeSet<String>,
    base: Option<&str>,
    cut: Option<&str>,
) -> Option<DoneLine> {
    let text = text?;
    let part = DonePart::Changelog;
    let Some(unreleased) = unreleased_entries(text) else {
        // Only a Keep a Changelog file (`## [1.2.0]` headings) promises an
        // [Unreleased] section; another tool's changelog is not judged.
        if !text.lines().any(|l| l.starts_with("## [")) {
            return Some(DoneLine::new(
                part,
                DoneState::CantTell,
                format!("{file} is not in Keep a Changelog form"),
            ));
        }
        return Some(
            DoneLine::new(
                part,
                DoneState::NotDone,
                format!("{file} has no [Unreleased] section"),
            )
            .with_fixes([format!(
                "add `## [Unreleased]` above the newest version in {file}"
            )]),
        );
    };
    // Right after a release is cut, its section holds the entries.
    let release = (unreleased == 0).then_some(cut).flatten().and_then(|v| {
        let h = format!("## [{}]", v.to_ascii_lowercase());
        section_entries(text, |l| l.starts_with(&h))
            .filter(|n| *n > 0)
            .map(|n| (v, n))
    });
    let (entries, section) = match release {
        Some((v, n)) => (n, format!("[{v}]")),
        None => (unreleased, "[Unreleased]".to_owned()),
    };
    let has = |n: usize| format!("{section} has {n} {}", plural(n, "entry", "entries"));
    let Some(base) = base else {
        return Some(if entries > 0 {
            DoneLine::new(part, DoneState::Done, has(entries))
        } else {
            DoneLine::new(
                part,
                DoneState::CantTell,
                "[Unreleased] is empty; no base to compare with",
            )
            .with_fixes(["speccoder done --base <branch or tag>"])
        });
    };
    let user: Vec<&String> = changed.iter().filter(|p| !is_aside(p, docs)).collect();
    if user.is_empty() {
        return Some(DoneLine::new(
            part,
            DoneState::Done,
            format!("nothing user-facing changed since {base}"),
        ));
    }
    // These changes need their own entry: one there from earlier work
    // does not cover them.
    if entries > 0 && changed.iter().any(|p| p == file) {
        return Some(DoneLine::new(part, DoneState::Done, has(entries)));
    }
    Some(
        DoneLine::new(
            part,
            DoneState::NotDone,
            format!(
                "{}; {} user-facing {} changed since {base}",
                if entries > 0 {
                    "[Unreleased] has no entry from these changes"
                } else {
                    "[Unreleased] is empty"
                },
                user.len(),
                plural(user.len(), "file", "files")
            ),
        )
        .with_findings(user.iter().map(|p| finding(p, None, "changed")).collect())
        .with_fixes([format!("add a line under `## [Unreleased]` in {file}")]),
    )
}

/// Prose only: front matter, fenced blocks and inline code become blank,
/// so line numbers stay.
#[must_use]
pub fn prose(text: &str) -> Vec<String> {
    static INLINE: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"`+[^`]*`+").ok());
    let mut out = Vec::new();
    let mut fence: Option<&str> = None;
    let mut front = false;
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if i == 0 && (t == "+++" || t == "---") {
            front = true;
            out.push(String::new());
            continue;
        }
        if front {
            front = !(t == "+++" || t == "---");
            out.push(String::new());
            continue;
        }
        if let Some(f) = fence {
            if t.starts_with(f) {
                fence = None;
            }
            out.push(String::new());
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = Some(if t.starts_with("```") { "```" } else { "~~~" });
            out.push(String::new());
            continue;
        }
        out.push(match &*INLINE {
            Some(r) => r.replace_all(line, "").into_owned(),
            None => line.to_owned(),
        });
    }
    out
}

/// The tone line: docs describe what is there.
#[must_use]
pub fn tone(pages: &[Page]) -> DoneLine {
    static RULES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
        let v = r"v?\d+\.\d+(\.\d+)?";
        [
            (format!(r"(?i)\bnew in {v}"), "version history"),
            (
                format!(r"(?i)\b(added|introduced|changed|removed|deprecated) in {v}"),
                "version history",
            ),
            (
                r"(?i)\b(since|as of) v\d+\.\d+".to_owned(),
                "version history",
            ),
            (r"(?i)\bcoming soon\b".to_owned(), "backlog"),
            (
                r"(?i)\bnot yet (supported|implemented|available)\b".to_owned(),
                "backlog",
            ),
            (
                r"(?i)\bin a (future|later|coming) (release|version)\b".to_owned(),
                "backlog",
            ),
            (r"(^|\s)(TODO|TBD|FIXME)(:|\s*$)".to_owned(), "backlog"),
            (r"\b20\d\d-\d\d-\d\d\b".to_owned(), "a date"),
        ]
        .into_iter()
        .filter_map(|(r, w)| Regex::new(&r).ok().map(|r| (r, w)))
        .collect()
    });
    // Link targets, addresses and HTML tags are not prose.
    static NOT_PROSE: LazyLock<Option<Regex>> =
        LazyLock::new(|| Regex::new(r"\]\([^)]*\)|[a-zA-Z][a-zA-Z0-9+.-]*://\S+|<[^>]+>").ok());
    let mut found = Vec::new();
    for p in pages {
        for (i, line) in prose(&p.text).iter().enumerate() {
            let line = NOT_PROSE
                .as_ref()
                .map_or_else(|| line.clone(), |r| r.replace_all(line, " ").into_owned());
            if let Some((m, why)) = RULES
                .iter()
                .find_map(|(r, w)| r.find(&line).map(|m| (m, *w)))
            {
                found.push(finding(
                    &p.path,
                    Some(i + 1),
                    format!("\"{}\" ({why})", m.as_str()),
                ));
            }
        }
    }
    if found.is_empty() {
        return DoneLine::new(
            DonePart::Tone,
            DoneState::Done,
            format!(
                "{} {} describe what is there",
                pages.len(),
                plural(pages.len(), "page", "pages")
            ),
        );
    }
    DoneLine::new(
        DonePart::Tone,
        DoneState::NotDone,
        format!(
            "{} {} of history, dates or backlog in docs",
            found.len(),
            plural(found.len(), "line", "lines")
        ),
    )
    .with_findings(found)
    .with_fixes([
        "describe what is there now; history belongs in the changelog, plans outside the docs",
    ])
}

/// The versions line: `None` when the version is the last release's (or
/// either is unknown).
#[must_use]
pub fn versions(pages: &[Page], version: Option<&str>, last: Option<&str>) -> Option<DoneLine> {
    let (version, last) = (version?, last?);
    if version == last {
        return None;
    }
    let pattern = Regex::new(&format!(r"(^|[^0-9.]){}($|[^0-9])", regex::escape(last))).ok()?;
    let mut found = Vec::new();
    for p in pages {
        for (i, line) in p.text.lines().enumerate() {
            if pattern.is_match(line) {
                found.push(finding(&p.path, Some(i + 1), format!("names {last}")));
            }
        }
    }
    let line = if found.is_empty() {
        DoneLine::new(
            DonePart::Versions,
            DoneState::Done,
            format!("no page names {last}"),
        )
    } else {
        DoneLine::new(
            DonePart::Versions,
            DoneState::NotDone,
            format!(
                "{} {} still {} {last}, the version is {version}",
                found.len(),
                plural(found.len(), "line", "lines"),
                plural(found.len(), "names", "name")
            ),
        )
        .with_findings(found)
        .with_fixes([format!("replace {last} with {version} on each line")])
    };
    Some(line)
}

/// The screenshots line: `None` without declarations.
#[must_use]
pub fn screenshots(shots: &[Shot]) -> Option<DoneLine> {
    if shots.is_empty() {
        return None;
    }
    let mut found = Vec::new();
    let mut stale = Vec::new();
    let mut files = 0;
    for s in shots {
        files += s.files.len();
        let what = if s.files.is_empty() {
            Some("no file matches it".to_owned())
        } else if s.shot_dirty {
            None
        } else if let Some(src) = s.dirty_sources.first() {
            Some(format!("{src} changed after it (not committed)"))
        } else {
            match (&s.newest_source, s.shot_time) {
                (Some((src, t)), Some(shot)) if *t > shot => {
                    Some(format!("{src} changed after it"))
                }
                _ => None,
            }
        };
        if let Some(what) = what {
            found.push(finding(&s.glob, None, what));
            stale.push(s.name.as_str());
        }
    }
    Some(if found.is_empty() {
        DoneLine::new(
            DonePart::Screenshots,
            DoneState::Done,
            format!("{files} {} current", plural(files, "shot", "shots")),
        )
    } else {
        DoneLine::new(
            DonePart::Screenshots,
            DoneState::NotDone,
            format!(
                "{} of {} {} older than what {}",
                found.len(),
                shots.len(),
                plural(shots.len(), "screenshot", "screenshots"),
                plural(found.len(), "it shows", "they show")
            ),
        )
        .with_findings(found)
        .with_fixes([format!("retake {} and commit", stale.join(", "))])
    })
}

/// GitHub's heading anchor: lowercase, punctuation dropped, each space a
/// hyphen.
#[must_use]
pub fn github_slug(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

/// zola's heading anchor (`slugify = "on"`): lowercase, every run of
/// anything not a letter or digit one hyphen, none at either end.
#[must_use]
pub fn zola_slug(heading: &str) -> String {
    let mut out = String::new();
    for c in heading.to_lowercase().chars() {
        let ascii = latin(c);
        if !ascii.is_empty() {
            out.push_str(ascii);
        } else if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

/// A lowercase Latin letter with a mark, as zola transliterates it.
const fn latin(c: char) -> &'static str {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'å' | 'ā' | 'ą' | 'ä' => "a",
        'æ' => "ae",
        'ç' | 'č' | 'ć' => "c",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => "e",
        'ì' | 'í' | 'î' | 'ï' => "i",
        'ñ' | 'ń' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ø' | 'ö' => "o",
        'œ' => "oe",
        'ù' | 'ú' | 'û' | 'ů' | 'ü' => "u",
        'ý' | 'ÿ' => "y",
        'ß' => "ss",
        'ł' => "l",
        'ś' | 'š' => "s",
        'ź' | 'ż' | 'ž' => "z",
        'ř' => "r",
        _ => "",
    }
}

/// A page's anchors: its headings' slugs (duplicates `-1`, `-2`) and any
/// `id="…"` it sets.
#[must_use]
pub fn anchors(text: &str, zola: bool) -> BTreeSet<String> {
    static ID: LazyLock<Option<Regex>> =
        LazyLock::new(|| Regex::new(r#"\b(?:id|name)="([^"]+)""#).ok());
    static CODE: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"`([^`]*)`").ok());
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut out = BTreeSet::new();
    for line in prose_keeping_inline(text) {
        if let Some(h) = line.trim_start().strip_prefix('#') {
            let h = h.trim_start_matches('#').trim();
            // `## Heading {#custom}`: the id is given.
            if let Some((_, id)) = h.strip_suffix('}').and_then(|h| h.rsplit_once("{#")) {
                out.insert(id.trim().to_owned());
                continue;
            }
            let h = CODE
                .as_ref()
                .map_or_else(|| h.to_owned(), |r| r.replace_all(h, "$1").into_owned());
            let slug = if zola { zola_slug(&h) } else { github_slug(&h) };
            let n = seen.entry(slug.clone()).or_default();
            out.insert(if *n == 0 { slug } else { format!("{slug}-{n}") });
            *n += 1;
        }
    }
    for c in ID.iter().flat_map(|r| r.captures_iter(text)) {
        out.insert(c[1].to_owned());
    }
    out
}

/// [`prose`] with inline code kept (headings may hold it).
fn prose_keeping_inline(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut fence = false;
    let mut front = false;
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if i == 0 && (t == "+++" || t == "---") {
            front = true;
            continue;
        }
        if front {
            front = !(t == "+++" || t == "---");
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
            continue;
        }
        if !fence {
            out.push(line.to_owned());
        }
    }
    out
}

/// A link found in a page: its line and its target as written.
#[must_use]
pub fn links_in(text: &str) -> Vec<(usize, String)> {
    static MD: LazyLock<Option<Regex>> =
        LazyLock::new(|| Regex::new(r"\]\(\s*<?([^)\s>]+)>?(?:\s+[^)]*)?\)").ok());
    static HTML: LazyLock<Option<Regex>> =
        LazyLock::new(|| Regex::new(r#"\b(?:href|src)="([^"]+)""#).ok());
    // A shortcode's arguments are the template's business.
    static TEMPLATE: LazyLock<Option<Regex>> =
        LazyLock::new(|| Regex::new(r"\{\{.*?\}\}|\{%.*?%\}").ok());
    let mut out = Vec::new();
    for (i, line) in prose(text).iter().enumerate() {
        let line = TEMPLATE
            .as_ref()
            .map_or_else(|| line.clone(), |r| r.replace_all(line, "").into_owned());
        for c in MD
            .iter()
            .chain(HTML.iter())
            .flat_map(|r| r.captures_iter(&line))
        {
            out.push((i + 1, c[1].to_owned()));
        }
    }
    out
}

/// Whether a target leaves the checkout: any scheme (`https:`, `mailto:`,
/// `data:`, `tel:`) or a protocol-relative address.
#[must_use]
pub fn is_external(target: &str) -> bool {
    static SCHEME: LazyLock<Option<Regex>> =
        LazyLock::new(|| Regex::new(r"^[a-zA-Z][a-zA-Z0-9+.-]*:").ok());
    target.starts_with("//") || SCHEME.as_ref().is_some_and(|r| r.is_match(target))
}

/// The links line, given how each target resolves: `resolve(page, target)`
/// is `Ok(None)` for a link that resolves, `Ok(Some(why))` for one that does
/// not, `Err(())` for an external one.
#[must_use]
pub fn links(
    pages: &[Page],
    resolve: impl Fn(&Page, &str) -> Result<Option<String>, ()>,
) -> DoneLine {
    let (mut checked, mut external) = (0, 0);
    let mut found = Vec::new();
    for p in pages {
        for (line, target) in links_in(&p.text) {
            match resolve(p, &target) {
                Err(()) => external += 1,
                Ok(None) => checked += 1,
                Ok(Some(why)) => {
                    checked += 1;
                    found.push(finding(&p.path, Some(line), format!("{target} — {why}")));
                }
            }
        }
    }
    let ext = if external > 0 {
        format!("; {external} external not checked")
    } else {
        String::new()
    };
    if found.is_empty() {
        return DoneLine::new(
            DonePart::Links,
            DoneState::Done,
            format!(
                "{checked} {} in {} pages resolve{ext}",
                plural(checked, "link", "links"),
                pages.len()
            ),
        );
    }
    DoneLine::new(
        DonePart::Links,
        DoneState::NotDone,
        format!(
            "{} dead {} of {checked}{ext}",
            found.len(),
            plural(found.len(), "link", "links")
        ),
    )
    .with_findings(found)
    .with_fixes(["point each link at a page and heading that exist"])
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(path: &str, text: &str) -> Page {
        Page {
            path: path.into(),
            text: text.into(),
        }
    }

    const CHANGELOG: &str =
        "# Changelog\n\n## [Unreleased]\n\n## [0.1.0] - 2026-01-01\n\n- first\n";

    #[test]
    fn an_empty_unreleased_with_a_source_change_is_not_done_and_says_where() {
        let docs = BTreeSet::new();
        let changed = vec!["src/lib.rs".to_owned(), "tests/a.rs".to_owned()];
        let l = changelog(
            "CHANGELOG.md",
            Some(CHANGELOG),
            &changed,
            &docs,
            Some("v0.1.0"),
            None,
        )
        .unwrap();
        assert_eq!(l.state, DoneState::NotDone);
        assert_eq!(l.findings.len(), 1);
        assert_eq!(l.findings[0].path, "src/lib.rs");
        assert!(l.fixes[0].contains("## [Unreleased]"), "{:?}", l.fixes);
    }

    #[test]
    fn only_tests_docs_and_ci_changing_needs_no_entry() {
        let docs = BTreeSet::from(["guide/intro.txt".to_owned()]);
        let changed: Vec<String> = [
            "tests/a.rs",
            "src/a_test.go",
            "web/x.test.ts",
            ".github/workflows/ci.yml",
            "README.md",
            "guide/intro.txt",
            "crates/x/tests/fixtures/y.json",
        ]
        .map(str::to_owned)
        .to_vec();
        let l = changelog(
            "CHANGELOG.md",
            Some(CHANGELOG),
            &changed,
            &docs,
            Some("main"),
            None,
        )
        .unwrap();
        assert_eq!(l.state, DoneState::Done, "{l:?}");
    }

    #[test]
    fn an_entry_is_done_and_a_missing_section_or_base_is_said() {
        let docs = BTreeSet::new();
        let filled = CHANGELOG.replace(
            "## [Unreleased]\n",
            "## [Unreleased]\n\n### Added\n\n- a thing\n",
        );
        let ch = vec!["src/a.rs".to_owned()];
        assert_eq!(
            changelog("C", Some(&filled), &ch, &docs, None, None)
                .unwrap()
                .state,
            DoneState::Done
        );
        let none = changelog(
            "C",
            Some("# Changelog\n## [0.1.0]\n"),
            &ch,
            &docs,
            None,
            None,
        )
        .unwrap();
        assert_eq!(none.state, DoneState::NotDone);
        let nobase = changelog("C", Some(CHANGELOG), &ch, &docs, None, None).unwrap();
        assert_eq!(nobase.state, DoneState::CantTell);
        assert!(changelog("C", None, &ch, &docs, None, None).is_none());
    }

    #[test]
    fn a_release_just_cut_is_judged_by_its_own_section() {
        let docs = BTreeSet::new();
        let cut = "# Changelog\n\n## [Unreleased]\n\n## [0.2.0] - 2026-02-01\n\n- a thing\n- another\n\n## [0.1.0] - 2026-01-01\n\n- first\n";
        let ch = vec!["src/a.rs".to_owned(), "CHANGELOG.md".to_owned()];
        let l = changelog(
            "CHANGELOG.md",
            Some(cut),
            &ch,
            &docs,
            Some("v0.1.0"),
            Some("0.2.0"),
        )
        .unwrap();
        assert_eq!(l.state, DoneState::Done, "{l:?}");
        assert_eq!(l.summary, "[0.2.0] has 2 entries");
        // A version already released is no cut: [Unreleased] must say it.
        let l = changelog("CHANGELOG.md", Some(cut), &ch, &docs, Some("v0.1.0"), None).unwrap();
        assert_eq!(l.state, DoneState::NotDone);
    }

    #[test]
    fn tone_flags_history_and_backlog_in_prose_only() {
        let text = "+++\ntitle = \"2026-01-01\"\n+++\nThis is new in 0.17.\n\n```\nTODO: in code\n```\nRun `coming soon` here.\nExport is coming soon.\nA TBD placeholder is fine; so is a step planned first.\nReleased 2026-11-16.\nTODO: write this\n";
        let l = tone(&[page("docs/a.md", text)]);
        assert_eq!(l.state, DoneState::NotDone);
        let lines: Vec<_> = l.findings.iter().map(|f| f.line.unwrap()).collect();
        assert_eq!(lines, [4, 10, 12, 13], "{:?}", l.findings);
        assert!(tone(&[page("a.md", "Plain words.\n")]).state == DoneState::Done);
    }

    #[test]
    fn versions_flags_the_last_release_only_after_a_bump() {
        let p = [page(
            "docs/i.md",
            "install 0.17.0\nnot 10.17.0 or 0.17.01\n",
        )];
        let l = versions(&p, Some("0.18.0"), Some("0.17.0")).unwrap();
        assert_eq!(l.state, DoneState::NotDone);
        assert_eq!(l.findings.len(), 1);
        assert_eq!(l.findings[0].line, Some(1));
        assert!(versions(&p, Some("0.17.0"), Some("0.17.0")).is_none());
        assert!(versions(&p, None, Some("0.17.0")).is_none());
    }

    #[test]
    fn a_shot_older_than_its_source_committed_or_not_is_stale() {
        let base = Shot {
            name: "trust".into(),
            glob: "s/trust-*.webp".into(),
            files: vec!["s/trust-light.webp".into()],
            shot_time: Some(100),
            ..Shot::default()
        };
        let current = Shot {
            newest_source: Some(("ui/T.svelte".into(), 90)),
            ..base.clone()
        };
        let later = Shot {
            newest_source: Some(("ui/T.svelte".into(), 120)),
            ..base.clone()
        };
        let dirty = Shot {
            dirty_sources: vec!["ui/T.svelte".into()],
            ..current.clone()
        };
        let retaken = Shot {
            shot_dirty: true,
            ..dirty.clone()
        };
        let missing = Shot {
            files: vec![],
            ..base.clone()
        };
        assert_eq!(
            screenshots(std::slice::from_ref(&current)).unwrap().state,
            DoneState::Done
        );
        assert_eq!(screenshots(&[retaken]).unwrap().state, DoneState::Done);
        let l = screenshots(&[later, dirty, missing]).unwrap();
        assert_eq!(l.state, DoneState::NotDone);
        assert_eq!(l.findings.len(), 3, "{:?}", l.findings);
        assert!(l.findings[1].what.contains("not committed"));
        assert_eq!(l.findings[0].path, "s/trust-*.webp");
        assert_eq!(l.fixes[0], "retake trust, trust, trust and commit");
        assert!(screenshots(&[]).is_none());
    }

    #[test]
    fn slugs_follow_the_site_and_github() {
        assert_eq!(zola_slug("`[hooks.<name>]`"), "hooks-name");
        assert_eq!(zola_slug("USAGE-CONFLICT"), "usage-conflict");
        assert_eq!(github_slug("🖥️ The app"), "-the-app");
        assert_eq!(github_slug("Hooks & gates"), "hooks--gates");
        let a = anchors(
            "# Intro\n## Intro\n```\n# not\n```\n<a id=\"x\"></a>\n",
            true,
        );
        assert_eq!(
            a,
            BTreeSet::from(["intro".into(), "intro-1".into(), "x".into()])
        );
    }

    #[test]
    fn links_are_found_outside_code_and_counted() {
        let text = "See [a](@/docs/a.md#x) and [b](b.md).\n`[c](no.md)`\n<img src=\"../s.webp\">\n[e](https://x.y)\n";
        let found: Vec<String> = links_in(text).into_iter().map(|(_, t)| t).collect();
        assert_eq!(found, ["@/docs/a.md#x", "b.md", "../s.webp", "https://x.y"]);
        let l = links(&[page("p.md", text)], |_, t| {
            if is_external(t) {
                Err(())
            } else if t == "b.md" {
                Ok(Some("no such file".into()))
            } else {
                Ok(None)
            }
        });
        assert_eq!(l.state, DoneState::NotDone);
        assert!(
            l.summary.contains("1 external not checked"),
            "{}",
            l.summary
        );
        assert_eq!(l.findings[0].line, Some(1));
    }
    #[test]
    fn review_findings_hold_no_false_alarm() {
        // Another tool's changelog is not judged; an older entry does not
        // cover new changes.
        let docs = BTreeSet::new();
        let ch = vec!["src/a.rs".to_owned()];
        let other = changelog(
            "C",
            Some("# Changes\n\n## 1.2.0\n- x\n"),
            &ch,
            &docs,
            Some("main"),
            None,
        )
        .unwrap();
        assert_eq!(other.state, DoneState::CantTell);
        let old = "## [Unreleased]\n\n- earlier\n\n## [0.1.0]\n";
        let l = changelog("C", Some(old), &ch, &docs, Some("main"), None).unwrap();
        assert_eq!(l.state, DoneState::NotDone, "{l:?}");
        assert!(l.summary.contains("no entry from these changes"));
        let touched = vec!["src/a.rs".to_owned(), "C".to_owned()];
        assert_eq!(
            changelog("C", Some(old), &touched, &docs, Some("main"), None)
                .unwrap()
                .state,
            DoneState::Done
        );
        assert!(unreleased_entries("## Unreleased\n- a\n").is_some());
        // Numbers and dated links are not history.
        let calm = "It finishes until 0.5 s. Python since 3.8 works. See [the post](https://x.y/2026-01-02/a).\n";
        assert_eq!(tone(&[page("a.md", calm)]).state, DoneState::Done);
        // Any scheme is external; shortcodes, ids and accents resolve.
        for t in [
            "data:image/svg+xml;base64,AA",
            "tel:+1",
            "javascript:void(0)",
        ] {
            assert!(is_external(t), "{t}");
        }
        assert!(links_in("{{ figure(src=\"img.png\") }}\n").is_empty());
        assert!(anchors("## Heading {#custom}\n", true).contains("custom"));
        assert_eq!(zola_slug("Über Ästhetik"), "uber-asthetik");
    }
}
