//! Shape: what a writing assistant changed in a person's words, decided by
//! code. The model proposes a requirement; this module computes which
//! words of the proposal are the person's and which were added, which of
//! the person's words were dropped, which sentences landed nowhere, which
//! added numbers and terms no assumption explains, and what in the text
//! makes it hard to test. Nothing here asks the model or believes it.

use serde::{Deserialize, Serialize};

/// Where a stretch of text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// The person wrote it (in the proposal), or it was kept (in theirs).
    Yours,
    /// The assistant added it.
    Added,
    /// The person wrote it and the proposal left it out.
    Dropped,
}

/// A stretch of text and its origin; the stretches of one text, in order,
/// are that text exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Span {
    /// The text, separators included.
    pub text: String,
    /// Where it came from.
    pub origin: Origin,
}

/// The proposal's stretches (yours or added) and the person's (yours or
/// dropped).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Diff {
    /// Over the proposal.
    pub proposal: Vec<Span>,
    /// Over the person's text.
    pub original: Vec<Span>,
}

/// Longest texts compared word by word; longer ones are marked whole.
const MAX_WORDS: usize = 3000;

/// A token: a word (letters and digits) or what lies between words.
fn tokens(text: &str) -> Vec<(&str, bool)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut word: Option<bool> = None;
    for (i, c) in text.char_indices() {
        let w = c.is_alphanumeric();
        match word {
            Some(prev) if prev != w => {
                out.push((&text[start..i], prev));
                start = i;
            }
            _ => {}
        }
        word = Some(w);
    }
    if start < text.len() {
        out.push((&text[start..], word.unwrap_or(false)));
    }
    out
}

/// A word as the diff compares it: lower case, a plural `s` dropped
/// (`tables` is `table`), so rewording does not read as rewriting.
fn stem(word: &str) -> String {
    let w = word.to_lowercase();
    match w.strip_suffix('s') {
        Some(base) if base.chars().count() > 2 && !base.ends_with('s') => base.to_owned(),
        _ => w,
    }
}

fn push(spans: &mut Vec<Span>, text: &str, origin: Origin) {
    match spans.last_mut() {
        Some(last) if last.origin == origin => last.text.push_str(text),
        _ if text.is_empty() => {}
        _ => spans.push(Span {
            text: text.to_owned(),
            origin,
        }),
    }
}

/// Which words of `proposal` are `original`'s (the longest common
/// subsequence of their words, case-insensitive) and which were added;
/// which of `original`'s words were dropped. Separators follow the word
/// before them.
#[must_use]
pub fn diff(original: &str, proposal: &str) -> Diff {
    let a = tokens(original);
    let b = tokens(proposal);
    let aw: Vec<(usize, String)> = a
        .iter()
        .enumerate()
        .filter(|(_, (_, w))| *w)
        .map(|(i, (t, _))| (i, stem(t)))
        .collect();
    let bw: Vec<(usize, String)> = b
        .iter()
        .enumerate()
        .filter(|(_, (_, w))| *w)
        .map(|(i, (t, _))| (i, stem(t)))
        .collect();
    let mut kept_a = vec![false; a.len()];
    let mut kept_b = vec![false; b.len()];
    if aw.len() <= MAX_WORDS && bw.len() <= MAX_WORDS {
        let (n, m) = (aw.len(), bw.len());
        let mut lcs = vec![0u16; (n + 1) * (m + 1)];
        let at = |i: usize, j: usize| i * (m + 1) + j;
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[at(i, j)] = if aw[i].1 == bw[j].1 {
                    lcs[at(i + 1, j + 1)] + 1
                } else {
                    lcs[at(i + 1, j)].max(lcs[at(i, j + 1)])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n && j < m {
            if aw[i].1 == bw[j].1 {
                kept_a[aw[i].0] = true;
                kept_b[bw[j].0] = true;
                i += 1;
                j += 1;
            } else if lcs[at(i + 1, j)] >= lcs[at(i, j + 1)] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    let spans = |toks: &[(&str, bool)], kept: &[bool], lost: Origin| {
        let mut out = Vec::new();
        let mut last = Origin::Yours;
        for (k, (t, w)) in toks.iter().enumerate() {
            let origin = if *w {
                last = if kept[k] { Origin::Yours } else { lost };
                last
            } else {
                last
            };
            push(&mut out, t, origin);
        }
        out
    };
    Diff {
        proposal: spans(&b, &kept_b, Origin::Added),
        original: spans(&a, &kept_a, Origin::Dropped),
    }
}

/// Words too common to tell where a sentence went.
const STOP: &[&str] = &[
    "the", "and", "for", "that", "this", "with", "from", "are", "was", "but", "not", "can",
    "should", "must", "will", "shall", "have", "has", "they", "them", "its", "into", "when",
    "then", "also", "just", "some", "any", "all", "our", "your", "you", "who", "what", "which",
    "there", "their", "would", "could", "maybe", "probably", "like", "stuff", "thing", "things",
    "user", "users", "system", "dont", "don", "doesnt", "doesn", "isnt", "isn", "cant", "wont",
    "won", "shouldnt", "make", "sure", "able",
];

fn content(text: &str) -> Vec<String> {
    tokens(text)
        .into_iter()
        .filter(|(t, w)| *w && t.chars().count() > 2)
        .map(|(t, _)| t.to_lowercase())
        .filter(|t| !STOP.contains(&t.as_str()))
        .collect()
}

/// The sentences of a person's text: split at `.`, `!`, `?` and line
/// breaks; list markers dropped; empty ones skipped.
#[must_use]
pub fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut flush = |cur: &mut String| {
        let s = cur
            .trim()
            .trim_start_matches(['-', '*', '•'])
            .trim()
            .to_owned();
        if s.chars().any(char::is_alphanumeric) {
            out.push(s);
        }
        cur.clear();
    };
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c == '\n' {
            flush(&mut cur);
            continue;
        }
        cur.push(*c);
        let ends =
            matches!(c, '.' | '!' | '?') && chars.get(i + 1).is_none_or(|n| n.is_whitespace());
        if ends {
            flush(&mut cur);
        }
    }
    flush(&mut cur);
    out
}

/// Where one of the person's sentences went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Coverage {
    /// The sentence, as the person wrote it.
    pub sentence: String,
    /// The proposal's lines (0-based) that carry its words.
    pub lines: Vec<usize>,
    /// No line carries it.
    pub dropped: bool,
}

/// Each sentence of `original` and the lines of `proposal` sharing at
/// least half of its content words (at least one); a sentence with no
/// content words (a greeting) is never dropped.
#[must_use]
pub fn coverage(original: &str, proposal: &str) -> Vec<Coverage> {
    let lines: Vec<Vec<String>> = proposal.lines().map(content).collect();
    sentences(original)
        .into_iter()
        .map(|sentence| {
            let words = content(&sentence);
            let need = words.len().div_ceil(2).max(1);
            let hits: Vec<usize> = lines
                .iter()
                .enumerate()
                .filter(|(_, l)| words.iter().filter(|w| l.contains(w)).count() >= need)
                .map(|(i, _)| i)
                .collect();
            Coverage {
                dropped: !words.is_empty() && hits.is_empty(),
                lines: hits,
                sentence,
            }
        })
        .collect()
}

/// What the assistant says it introduced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Assumption {
    /// The proposal's words that carry it (`up to 10 000 rows`).
    pub words: String,
    /// What the person wrote that it reads into, when anything (`fast`).
    #[serde(default)]
    pub from: String,
    /// The question to ask instead of assuming it.
    pub question: String,
}

/// Added numbers, quantities and quoted terms that no assumption's words
/// contain: what the assistant introduced without saying so.
#[must_use]
pub fn unexplained(diff: &Diff, assumptions: &[Assumption]) -> Vec<String> {
    // Compared without case, spaces or punctuation: "10,000 rows" is
    // "10 000 rows".
    let bare = |t: &str| -> String {
        t.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let covered = |term: &str| {
        let t = bare(term);
        assumptions.iter().any(|a| bare(&a.words).contains(&t))
    };
    let mut out: Vec<String> = Vec::new();
    for s in diff.proposal.iter().filter(|s| s.origin == Origin::Added) {
        let text = s.text.as_str();
        let words: Vec<(usize, usize)> = tokens(text)
            .into_iter()
            .filter(|(_, w)| *w)
            .map(|(t, _)| {
                let at = t.as_ptr() as usize - text.as_ptr() as usize;
                (at, at + t.len())
            })
            .collect();
        let gap = |a: (usize, usize), b: (usize, usize)| &text[a.1..b.0];
        let mut terms: Vec<String> = Vec::new();
        // A number as written, with the word after it (`2 seconds`,
        // `10,000 rows`, `1.5 s`): the exact text, so it can be found.
        let mut k = 0;
        while k < words.len() {
            let w = &text[words[k].0..words[k].1];
            if w.chars().any(|c| c.is_ascii_digit()) {
                let start = words[k].0;
                let mut end = words[k].1;
                while k + 1 < words.len()
                    && text[words[k + 1].0..words[k + 1].1]
                        .chars()
                        .all(|c| c.is_ascii_digit())
                    && matches!(
                        gap(words[k], words[k + 1]),
                        " " | "," | "." | "\u{a0}" | "\u{202f}"
                    )
                {
                    k += 1;
                    end = words[k].1;
                }
                if k + 1 < words.len()
                    && text[words[k + 1].0..words[k + 1].1]
                        .chars()
                        .all(char::is_alphabetic)
                    && matches!(gap(words[k], words[k + 1]), " " | "-" | "\u{a0}")
                {
                    k += 1;
                    end = words[k].1;
                }
                terms.push(text[start..end].to_owned());
            }
            k += 1;
        }
        // A quoted or code term (`"admin"`, `` `--force` ``).
        for q in ['"', '`', '“'] {
            let close = if q == '“' { '”' } else { q };
            let mut rest = text;
            while let Some(i) = rest.find(q) {
                let after = &rest[i + q.len_utf8()..];
                let Some(j) = after.find(close) else { break };
                if j > 0 {
                    terms.push(after[..j].to_owned());
                }
                rest = &after[j + close.len_utf8()..];
            }
        }
        // A number inside a quoted term is that term (`utf-8`, not `8`).
        let whole = terms.clone();
        terms.retain(|t| !whole.iter().any(|o| o != t && o.contains(t.as_str())));
        for t in terms {
            if !covered(&t) && !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

/// A writing-check hit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Finding {
    /// `vague`, `escape_clause`, `open_ended`, `no_obligation`,
    /// `when_without_then`, `no_failure_path`.
    pub rule: String,
    /// The words it is about (empty for the whole text).
    pub words: String,
    /// Why it makes the requirement hard to test, in a line.
    pub reason: String,
    /// What to write instead.
    pub fix: String,
}

/// Vague words: each with the question that makes it testable.
const VAGUE: &[(&str, &str)] = &[
    ("fast", "how fast? a number and a unit (\"within 2 s\")"),
    ("quick", "how quick? a number and a unit"),
    ("quickly", "how quickly? a number and a unit"),
    ("slow", "how slow is too slow? a number and a unit"),
    ("large", "how large? a size or a count"),
    ("big", "how big? a size or a count"),
    ("small", "how small? a size or a count"),
    ("many", "how many? a number"),
    ("few", "how few? a number"),
    ("several", "how many? a number"),
    ("some", "which ones, or how many?"),
    ("easy", "easy for whom, measured how? name the steps"),
    ("simple", "simple how? name what it does and does not do"),
    ("intuitive", "measured how? name the behaviour"),
    ("user-friendly", "name the behaviour a person sees"),
    ("friendly", "name the behaviour a person sees"),
    (
        "robust",
        "robust against what? name the failure and the response",
    ),
    (
        "secure",
        "secure against what? name the threat and the behaviour",
    ),
    ("efficient", "how efficient? a number and a unit"),
    ("flexible", "flexible how? name the variations"),
    ("appropriate", "which, exactly? name it"),
    ("reasonable", "what is reasonable? a number or a rule"),
    ("normally", "and otherwise? name both cases"),
    ("usually", "and otherwise? name both cases"),
    ("probably", "decide it, or ask it as an open question"),
    ("maybe", "decide it, or ask it as an open question"),
    ("stuff", "name the things"),
    ("things", "name the things"),
    ("better", "better than what, by how much?"),
];

const ESCAPE: &[&str] = &[
    "as appropriate",
    "where possible",
    "if possible",
    "if practicable",
    "as needed",
    "as required",
    "if necessary",
];

const OPEN_ENDED: &[&str] = &[
    "etc",
    "and so on",
    "and so forth",
    "including but not limited to",
    "or similar",
];

/// Words that make a scenario a failure path.
const FAILURE: &[&str] = &[
    "fail",
    "fails",
    "failed",
    "failure",
    "error",
    "errors",
    "invalid",
    "reject",
    "rejects",
    "rejected",
    "refuse",
    "refuses",
    "refused",
    "missing",
    "empty",
    "cannot",
    "not",
    "denied",
    "unavailable",
    "timeout",
    "malformed",
    "exceeds",
    "without",
];

fn find_phrase(lower: &str, phrase: &str) -> bool {
    lower.match_indices(phrase).any(|(i, _)| {
        let before = lower[..i].chars().next_back();
        let after = lower[i + phrase.len()..].chars().next();
        before.is_none_or(|c| !c.is_alphanumeric()) && after.is_none_or(|c| !c.is_alphanumeric())
    })
}

/// The writing check of `text`, a requirement (with its scenarios) when
/// `requirement`: vague words, escape clauses, open-ended lists; for a
/// requirement also a missing obligation word (`MUST`/`SHALL`), a *When*
/// without a *Then*, and scenarios with no failure path. A fixed list,
/// model-free.
#[must_use]
pub fn check(text: &str, requirement: bool) -> Vec<Finding> {
    let lower = text.to_lowercase();
    let mut out = Vec::new();
    for (word, fix) in VAGUE {
        if find_phrase(&lower, word) {
            out.push(Finding {
                rule: "vague".to_owned(),
                words: (*word).to_owned(),
                reason: "cannot be tested as written".to_owned(),
                fix: (*fix).to_owned(),
            });
        }
    }
    for p in ESCAPE {
        if find_phrase(&lower, p) {
            out.push(Finding {
                rule: "escape_clause".to_owned(),
                words: (*p).to_owned(),
                reason: "lets any behaviour pass".to_owned(),
                fix: "say when it applies, or drop it".to_owned(),
            });
        }
    }
    for p in OPEN_ENDED {
        if find_phrase(&lower, p) {
            out.push(Finding {
                rule: "open_ended".to_owned(),
                words: (*p).to_owned(),
                reason: "leaves the list open: the rest cannot be tested".to_owned(),
                fix: "name every item".to_owned(),
            });
        }
    }
    if !requirement {
        return out;
    }
    let obliges = ["must", "shall"].iter().any(|w| find_phrase(&lower, w));
    if !obliges {
        out.push(Finding {
            rule: "no_obligation".to_owned(),
            words: String::new(),
            reason: "no MUST or SHALL: it reads as a wish, not a rule".to_owned(),
            fix: "say who MUST do what (\"The export MUST …\")".to_owned(),
        });
    }
    let scenario_lines: Vec<&str> = lower
        .lines()
        .filter(|l| find_phrase(l, "when") || find_phrase(l, "then") || l.contains("scenario"))
        .collect();
    let whens = lower.lines().filter(|l| find_phrase(l, "when")).count();
    let thens = lower.lines().filter(|l| find_phrase(l, "then")).count();
    if whens > thens {
        out.push(Finding {
            rule: "when_without_then".to_owned(),
            words: "When".to_owned(),
            reason: "a When without a Then: nothing to check".to_owned(),
            fix: "add what must happen: \"Then …\"".to_owned(),
        });
    }
    let failure = scenario_lines
        .iter()
        .any(|l| FAILURE.iter().any(|w| find_phrase(l, w)));
    if thens > 0 && !failure {
        out.push(Finding {
            rule: "no_failure_path".to_owned(),
            words: String::new(),
            reason: "every scenario is a success path".to_owned(),
            fix: "add one: what if it fails, or the input is wrong?".to_owned(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(spans: &[Span], origin: Origin) -> String {
        spans
            .iter()
            .filter(|s| s.origin == origin)
            .map(|s| s.text.trim())
            .collect::<Vec<_>>()
            .join("|")
    }

    #[test]
    fn the_diff_marks_added_and_dropped_words_and_keeps_every_byte() {
        let original = "users should be able to export stuff fast, csv probably";
        let proposal = "Users MUST be able to export the table as CSV within 2 seconds.";
        let d = diff(original, proposal);
        assert_eq!(
            d.proposal
                .iter()
                .map(|s| s.text.as_str())
                .collect::<String>(),
            proposal
        );
        assert_eq!(
            d.original
                .iter()
                .map(|s| s.text.as_str())
                .collect::<String>(),
            original
        );
        assert_eq!(
            text(&d.proposal, Origin::Added),
            "MUST|the table as|within 2 seconds."
        );
        assert_eq!(
            text(&d.original, Origin::Dropped),
            "should|stuff fast,|probably"
        );
    }

    #[test]
    fn every_sentence_lands_somewhere_or_is_dropped() {
        let original = "Export the table to CSV. Keep the header row!\nAlso it must never delete the source file.";
        let proposal =
            "The export MUST write the table as CSV.\nThe header row MUST be the first line.";
        let c = coverage(original, proposal);
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].lines, [0]);
        assert_eq!(c[1].lines, [1]);
        assert!(c[2].dropped, "{c:?}");
        // Said differently but carried: a negation is not what it is about.
        let c = coverage(
            "dont break the header thing",
            "The tool MUST preserve the header row.",
        );
        assert!(!c[0].dropped, "{c:?}");
    }

    #[test]
    fn an_added_number_or_quoted_term_needs_an_assumption() {
        let d = diff(
            "export it fast",
            "The export MUST finish within 2 seconds for up to 10 000 rows as \"utf-8\".",
        );
        let declared = [Assumption {
            words: "up to 10 000 rows".to_owned(),
            from: String::new(),
            question: "How many rows?".to_owned(),
        }];
        assert_eq!(unexplained(&d, &declared), ["2 seconds", "utf-8"]);
        assert!(unexplained(&diff("finish in 2 seconds", "finish in 2 seconds"), &[]).is_empty());
        // As written, so it can be found and replaced; covered across
        // punctuation.
        let d = diff(
            "export",
            "export up to 10,000 rows in 1.5 s with a 2-second timeout",
        );
        assert_eq!(unexplained(&d, &[]), ["10,000 rows", "1.5 s", "2-second"]);
        let declared = [Assumption {
            words: "up to 10 000 rows".to_owned(),
            from: String::new(),
            question: String::new(),
        }];
        assert_eq!(unexplained(&d, &declared), ["1.5 s", "2-second"]);
    }

    #[test]
    fn the_check_names_vague_words_and_what_a_requirement_lacks() {
        let f = check(
            "The export should be fast and handle large files etc.",
            true,
        );
        let rules: Vec<(&str, &str)> = f
            .iter()
            .map(|f| (f.rule.as_str(), f.words.as_str()))
            .collect();
        assert_eq!(
            rules,
            [
                ("vague", "fast"),
                ("vague", "large"),
                ("open_ended", "etc"),
                ("no_obligation", "")
            ]
        );
        // Not inside other words: "breakfast" is not "fast".
        assert!(check("A breakfast menu MUST be shown.", true).is_empty());
        let happy = "The export MUST write CSV.\nWhen the person exports\nThen a file is written";
        assert_eq!(
            check(happy, true)
                .iter()
                .map(|f| f.rule.as_str())
                .collect::<Vec<_>>(),
            ["no_failure_path"]
        );
        let both = "The export MUST write CSV.\nWhen the person exports\nThen a file is written\nWhen the table is empty\nThen the export fails with a message";
        assert!(check(both, true).is_empty());
        assert_eq!(
            check("When it runs", true)
                .iter()
                .filter(|f| f.rule == "when_without_then")
                .count(),
            1
        );
        // A note is not a requirement: only the words are checked.
        assert!(check("try the faster path first", false).is_empty());
    }
}
