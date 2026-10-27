//! Is a new feature's description close to a requirement the project
//! already has? A deterministic text similarity — normalised tokens, a
//! fixed stop-word list, a suffix stemmer, TF-IDF weights over the
//! project's requirements and cosine — so the answer is the same on every
//! run and explains itself by the terms it shares. A hint: it never
//! blocks, and no model produces or suppresses a match.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

/// A score at or above this is *close*.
pub const CLOSE: f64 = 0.3;
/// A score at or above this is *very close*.
pub const VERY_CLOSE: f64 = 0.6;
/// At most this many matches.
pub const MAX_MATCHES: usize = 3;
/// At most this many shared terms per match.
pub const MAX_SHARED: usize = 4;
/// The corpus is limited to the requirements of this many units, the most
/// recently changed first.
pub const MAX_UNITS: usize = 2000;

/// Words that carry no meaning of their own here: English function words
/// and the words every requirement uses.
const STOP: &[&str] = &[
    "a",
    "about",
    "above",
    "after",
    "again",
    "against",
    "all",
    "also",
    "am",
    "an",
    "and",
    "any",
    "are",
    "as",
    "at",
    "be",
    "because",
    "been",
    "before",
    "being",
    "below",
    "between",
    "both",
    "but",
    "by",
    "can",
    "cannot",
    "could",
    "did",
    "do",
    "does",
    "doing",
    "down",
    "during",
    "each",
    "either",
    "else",
    "even",
    "ever",
    "every",
    "few",
    "for",
    "from",
    "further",
    "get",
    "gets",
    "given",
    "had",
    "has",
    "have",
    "having",
    "he",
    "her",
    "here",
    "hers",
    "him",
    "his",
    "how",
    "if",
    "in",
    "into",
    "is",
    "it",
    "its",
    "itself",
    "just",
    "let",
    "lets",
    "may",
    "me",
    "might",
    "more",
    "most",
    "much",
    "my",
    "neither",
    "no",
    "nor",
    "not",
    "now",
    "of",
    "off",
    "on",
    "once",
    "one",
    "only",
    "or",
    "other",
    "our",
    "ours",
    "out",
    "over",
    "own",
    "per",
    "same",
    "shall",
    "she",
    "so",
    "some",
    "such",
    "than",
    "that",
    "the",
    "their",
    "theirs",
    "them",
    "then",
    "there",
    "these",
    "they",
    "this",
    "those",
    "through",
    "to",
    "too",
    "under",
    "until",
    "up",
    "upon",
    "us",
    "very",
    "via",
    "was",
    "we",
    "were",
    "what",
    "when",
    "where",
    "whether",
    "which",
    "while",
    "who",
    "whom",
    "why",
    "will",
    "with",
    "within",
    "without",
    "would",
    "yet",
    "you",
    "your",
    "yours",
    "able",
    "allow",
    "allows",
    "allowed",
    "must",
    "should",
    "system",
    "user",
    "users",
    "feature",
    "requirement",
    "new",
    "want",
    "wants",
    "need",
    "needs",
    "make",
    "makes",
    "use",
    "uses",
    "using",
    "way",
    "ways",
    "etc",
    "e",
    "g",
    "i",
    "ie",
    "eg",
];

/// The stem of a lowercase word: `ies`→`y`, then one of `es`, `s`,
/// `ed`, `ing`, `ly` stripped where enough of the word remains.
fn stem(word: &str) -> String {
    let n = word.chars().count();
    if n > 4
        && let Some(base) = word.strip_suffix("ies")
    {
        return format!("{base}y");
    }
    for (suffix, keep) in [("ing", 4), ("ed", 3), ("ly", 3), ("es", 3), ("s", 3)] {
        if let Some(base) = word.strip_suffix(suffix)
            && base.chars().count() >= keep
            && !(suffix == "s" && base.ends_with('s'))
        {
            // `es` only after a sibilant (`boxes`, `matches`); else `s`.
            if suffix == "es"
                && !["s", "x", "z", "ch", "sh"]
                    .iter()
                    .any(|e| base.ends_with(e))
            {
                continue;
            }
            return base.to_owned();
        }
    }
    word.to_owned()
}

/// The terms of `text`: NFKC-normalised, lowercased runs of letters and
/// digits of two characters or more, stop words dropped, stemmed.
#[must_use]
pub fn tokenize(text: &str) -> Vec<String> {
    let normal: String = text.nfkc().collect::<String>().to_lowercase();
    normal
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 2 && !STOP.contains(w))
        .map(stem)
        .filter(|w| w.chars().count() >= 2 && !STOP.contains(&w.as_str()))
        .collect()
}

/// One requirement of the corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    /// The unit's directory name or change id (`003-reports`).
    pub unit: String,
    /// Its identifier (`FR-004`) or the format's requirement title.
    pub id: String,
    /// The requirement as written.
    pub text: String,
    /// Where it is written, for a comment anchored to it: the document
    /// (project-relative), the line (1-based) and that line verbatim.
    pub at: Option<Place>,
}

/// Where a requirement is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Place {
    /// The document, project-relative.
    pub path: String,
    /// The line, 1-based.
    pub line: u32,
    /// That line, verbatim.
    pub quote: String,
}

/// How close a match is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SimilarLevel {
    /// At or above [`CLOSE`].
    Close,
    /// At or above [`VERY_CLOSE`].
    VeryClose,
}

impl SimilarLevel {
    /// The word a person reads (`very close`).
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Close => "close",
            Self::VeryClose => "very close",
        }
    }
}

/// A requirement close to the description.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SimilarMatch {
    /// The unit (`003-reports`).
    pub unit: String,
    /// The requirement's identifier (`FR-004`).
    pub requirement: String,
    /// The requirement as written.
    pub text: String,
    /// Cosine in [0, 1], rounded to two places.
    pub score: f64,
    /// `close` or `very_close`.
    pub level: SimilarLevel,
    /// The terms it shares with the description, the weightiest first.
    pub shared: Vec<String>,
    /// Where it is written.
    pub at: Option<Place>,
}

/// The unit's number (`003-reports` → 3), for ordering ties.
fn unit_number(unit: &str) -> u64 {
    unit.chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap_or(u64::MAX)
}

type Vector = BTreeMap<String, f64>;

fn counts(terms: &[String]) -> BTreeMap<String, f64> {
    let mut m = BTreeMap::new();
    for t in terms {
        *m.entry(t.clone()).or_insert(0.0) += 1.0;
    }
    m
}

fn norm(v: &Vector) -> f64 {
    v.values().map(|w| w * w).sum::<f64>().sqrt()
}

/// The requirements of `corpus` at or above [`CLOSE`] to `query`, at most
/// [`MAX_MATCHES`], by score, then unit number, then identifier. The same
/// input gives the same output; an empty query gives none.
#[must_use]
pub fn matches(query: &str, corpus: &[Requirement]) -> Vec<SimilarMatch> {
    let q_terms = tokenize(query);
    if q_terms.is_empty() || corpus.is_empty() {
        return Vec::new();
    }
    let docs: Vec<BTreeMap<String, f64>> =
        corpus.iter().map(|r| counts(&tokenize(&r.text))).collect();
    let q_counts = counts(&q_terms);
    // Document frequencies over the corpus and the query.
    let mut df: BTreeMap<&str, f64> = BTreeMap::new();
    for d in docs.iter().chain(std::iter::once(&q_counts)) {
        for t in d.keys() {
            *df.entry(t.as_str()).or_insert(0.0) += 1.0;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let n = (docs.len() + 1) as f64;
    let idf = |t: &str| ((1.0 + n) / (1.0 + df.get(t).copied().unwrap_or(0.0))).ln() + 1.0;
    let weigh = |c: &BTreeMap<String, f64>| -> Vector {
        c.iter().map(|(t, tf)| (t.clone(), tf * idf(t))).collect()
    };
    let q = weigh(&q_counts);
    let qn = norm(&q);
    let mut out: Vec<(f64, &Requirement, Vec<String>)> = Vec::new();
    for (r, d) in corpus.iter().zip(&docs) {
        let v = weigh(d);
        let vn = norm(&v);
        if vn == 0.0 || qn == 0.0 {
            continue;
        }
        let mut shared: Vec<(f64, &str)> = q
            .iter()
            .filter_map(|(t, w)| v.get(t).map(|dw| (w * dw, t.as_str())))
            .collect();
        let dot: f64 = shared.iter().map(|(p, _)| p).sum();
        let score = (dot / (qn * vn)).clamp(0.0, 1.0);
        if score < CLOSE {
            continue;
        }
        shared.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        let shared = shared
            .into_iter()
            .take(MAX_SHARED)
            .map(|(_, t)| t.to_owned())
            .collect();
        out.push((score, r, shared));
    }
    out.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| unit_number(&a.1.unit).cmp(&unit_number(&b.1.unit)))
            .then_with(|| a.1.unit.cmp(&b.1.unit))
            .then_with(|| a.1.id.cmp(&b.1.id))
    });
    out.into_iter()
        .take(MAX_MATCHES)
        .map(|(score, r, shared)| SimilarMatch {
            unit: r.unit.clone(),
            requirement: r.id.clone(),
            text: r.text.clone(),
            score: (score * 100.0).round() / 100.0,
            level: if score >= VERY_CLOSE {
                SimilarLevel::VeryClose
            } else {
                SimilarLevel::Close
            },
            shared,
            at: r.at.clone(),
        })
        .collect()
}

/// The terms two texts share, sorted (for a symmetric check).
#[must_use]
pub fn shared_terms(a: &str, b: &str) -> BTreeSet<String> {
    let a: BTreeSet<String> = tokenize(a).into_iter().collect();
    tokenize(b).into_iter().filter(|t| a.contains(t)).collect()
}

/// The `similar` payload: what was compared and what is close.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SimilarReport {
    /// The description, as given.
    pub query: String,
    /// How many requirements it was compared with.
    pub compared: usize,
    /// Whether the corpus was limited to the [`MAX_UNITS`] most recently
    /// changed units.
    pub limited: bool,
    /// The close ones, the closest first.
    pub matches: Vec<SimilarMatch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems_fold_the_usual_suffixes() {
        assert_eq!(stem("reports"), "report");
        assert_eq!(stem("exporting"), "export");
        assert_eq!(stem("exported"), "export");
        assert_eq!(stem("categories"), "category");
        assert_eq!(stem("boxes"), "box");
        assert_eq!(stem("class"), "class");
        assert_eq!(stem("quickly"), "quick");
        assert_eq!(stem("is"), "is");
    }

    #[test]
    fn tokens_are_normalised_and_stop_words_dropped() {
        assert_eq!(
            tokenize("Users MUST be able to export Reports as ＣＳＶ"),
            vec!["export", "report", "csv"]
        );
        assert!(tokenize("the a of").is_empty());
    }
}
