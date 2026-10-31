//! The ladder: where a unit of work came from. One provenance
//! record per subject, written once. This build knows the variants
//! a bug writes; later rungs add theirs as variants.

use serde::{Deserialize, Serialize};

use crate::actions::Via;

/// Where a unit or a requirement came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Origin {
    /// Recorded as a bug (`speccoder bug`).
    Bug {
        /// The unit.
        unit: String,
        /// The requirement.
        requirement: String,
        /// The task.
        task: String,
        /// The triad's digest.
        triad_sha256: String,
        /// Who.
        by: String,
        /// Through what.
        via: Via,
        /// Unix seconds.
        at: u64,
    },
    /// From a GitHub issue.
    Issue {
        /// Its number.
        number: u64,
        /// `owner/repo`.
        repo: Option<String>,
        /// The digest of its context-manifest entry.
        context_sha256: Option<String>,
    },
    /// A unit started from a quick change or bug that grew (promotion).
    PromotedFrom {
        /// The lower unit.
        from_unit: String,
        /// Its requirement, carried verbatim.
        requirement: String,
        /// The task that showed the signals.
        task: String,
        /// The requirement's words' digest when promoted.
        requirement_sha256: String,
        /// The Ask conversations carried, `id` and digest.
        conversations: Vec<(String, String)>,
        /// The signals that held.
        signals: Vec<String>,
        /// Who.
        by: String,
        /// Through what.
        via: Via,
        /// Unix seconds.
        at: u64,
    },
    /// Started from an Explore session's brief, verbatim.
    ExploredFrom {
        /// The session (`ex-…`).
        session: String,
        /// The brief's digest (hex SHA-256 of its body).
        digest: String,
        /// Who started it.
        by: String,
        /// Through what.
        via: Via,
        /// Unix seconds.
        at: u64,
    },
    /// Suggested in an Ask conversation and confirmed by a person.
    Asked {
        /// The conversation (`c-…`).
        conversation: String,
        /// The turn whose proposal it was, when known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn: Option<u32>,
        /// Unix seconds.
        at: u64,
    },
}

/// The origin a proposal confirmed from Ask records: `asked` is the
/// conversation, or `conversation#turn` for the turn that proposed it.
#[must_use]
pub fn asked(asked: &str, at: u64) -> Origin {
    let (conversation, turn) = match asked.split_once('#') {
        Some((c, t)) => (c, t.parse().ok()),
        None => (asked, None),
    };
    Origin::Asked {
        conversation: conversation.to_owned(),
        turn,
        at,
    }
}

impl Origin {
    /// One line for `status` and the trace.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Bug {
                requirement,
                by,
                via,
                ..
            } => format!("bug {requirement}, recorded by {by} ({via})"),
            Self::Issue { number, repo, .. } => match repo {
                Some(r) => format!("issue {r}#{number}"),
                None => format!("issue #{number}"),
            },
            Self::Asked { conversation, .. } => format!("asked in conversation {conversation}"),
            Self::ExploredFrom {
                session, digest, ..
            } => format!(
                "explored in {session}, brief {}",
                digest.get(..12).unwrap_or(digest)
            ),
            Self::PromotedFrom {
                from_unit,
                requirement,
                by,
                ..
            } => format!("promoted from {from_unit}/{requirement} by {by}"),
        }
    }
}

/// The rung a unit stands on: a quick change (L1), a bug (L2) or
/// a feature. A quick-change or bug unit's pipeline authors and confirms
/// its own acceptance, so the review writes no tests for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rung {
    /// A quick change's standing unit (`speccoder do`).
    Quick,
    /// A bug's home: a standing bug-fix unit or a bug of the format's own
    /// bug structure.
    Bug,
    /// Any other unit.
    Feature,
}

impl Rung {
    /// Its recorded name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Bug => "bug",
            Self::Feature => "feature",
        }
    }
}

/// The rung of `unit`, from the standing units the format names for the
/// project (`(unit, rung)`): a standing unit, or a unit under one
/// (`<standing>/<slug>`), is on its rung; every other unit is a feature.
#[must_use]
pub fn rung_of(unit: &str, standing: &[(String, Rung)]) -> Rung {
    standing
        .iter()
        .find(|(name, _)| {
            unit == name
                || unit
                    .strip_prefix(name.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        })
        .map_or(Rung::Feature, |(_, r)| *r)
}

/// What promotion signals are computed from: facts speccoder recorded,
/// never a model's opinion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SignalInput {
    /// Files the task's change touched.
    pub files_changed: u64,
    /// Packages (manifest roots) among them.
    pub packages_changed: u64,
    /// The causes of its failed attempts, one per attempt.
    pub red_causes: Vec<String>,
    /// Ambiguity markers in its requirement's words.
    pub ambiguity_markers: u64,
    /// Files among them that decide verdicts (the checks' configuration).
    pub judge_paths: Vec<String>,
    /// `[ladder] promote_files`.
    pub promote_files: u64,
}

/// A promotion signal that holds, with what was measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Signal {
    /// `S1` (size), `S2` (repeated distinct failures), `S3` (ambiguous
    /// words), `S4` (touches what judges).
    pub id: String,
    /// What it measured, in a line.
    pub text: String,
    /// The measured value.
    pub value: u64,
    /// The threshold it passed.
    pub threshold: u64,
}

/// The signals that hold for `i`: a quick change or bug that grew past
/// one requirement's ceremony. Pure and deterministic.
#[must_use]
pub fn signals(i: &SignalInput) -> Vec<Signal> {
    let mut out = Vec::new();
    let sig = |id: &str, text: String, value: u64, threshold: u64| Signal {
        id: id.to_owned(),
        text,
        value,
        threshold,
    };
    if i.files_changed > i.promote_files {
        out.push(sig(
            "S1",
            format!(
                "{} files changed (more than {})",
                i.files_changed, i.promote_files
            ),
            i.files_changed,
            i.promote_files,
        ));
    } else if i.packages_changed > 1 {
        out.push(sig(
            "S1",
            format!("{} packages changed (more than 1)", i.packages_changed),
            i.packages_changed,
            1,
        ));
    }
    let mut causes: Vec<&str> = i.red_causes.iter().map(String::as_str).collect();
    causes.sort_unstable();
    causes.dedup();
    if causes.len() >= 2 {
        out.push(sig(
            "S2",
            format!("{} failed attempts, each for another reason", causes.len()),
            causes.len() as u64,
            2,
        ));
    }
    if i.ambiguity_markers > 0 {
        out.push(sig(
            "S3",
            format!(
                "{} ambiguous {} in the requirement",
                i.ambiguity_markers,
                if i.ambiguity_markers == 1 {
                    "word"
                } else {
                    "words"
                }
            ),
            i.ambiguity_markers,
            0,
        ));
    }
    if !i.judge_paths.is_empty() {
        out.push(sig(
            "S4",
            format!("changes what judges: {}", i.judge_paths.join(", ")),
            i.judge_paths.len() as u64,
            0,
        ));
    }
    out
}

/// The subject a bug's origin is recorded for: `<unit>/<requirement>`.
#[must_use]
pub fn subject(unit: &str, requirement: &str) -> String {
    format!("{unit}/{requirement}")
}

/// A rung the composer can open for a description: a bug fix, a quick
/// change, a feature, or a question (`ask`); `none` only for an empty text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RungPick {
    /// A wrong result of existing behaviour (`speccoder bug`).
    Bug,
    /// One requirement, judged like a task (`speccoder do`).
    Quick,
    /// A spec the format's skill writes (`speccoder spec specify`).
    Feature,
    /// A question: a read-only answer.
    Ask,
    /// An idea still weighing its approaches (`speccoder spec explore`).
    Explore,
    /// Nothing to suggest: the text is empty.
    None,
}

impl RungPick {
    /// Every rung, in the composer's order.
    pub const ORDER: [Self; 5] = [
        Self::Bug,
        Self::Quick,
        Self::Feature,
        Self::Explore,
        Self::Ask,
    ];

    /// Its name as serialized (`quick`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bug => "bug",
            Self::Quick => "quick",
            Self::Feature => "feature",
            Self::Ask => "ask",
            Self::Explore => "explore",
            Self::None => "none",
        }
    }

    /// How the composer names it (`Bug fix`).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Bug => "Bug fix",
            Self::Quick => "Quick change",
            Self::Feature => "Feature",
            Self::Ask => "Just ask",
            Self::Explore => "Explore",
            Self::None => "Nothing",
        }
    }

    /// Its one-line ceremony.
    #[must_use]
    pub const fn ceremony(self) -> &'static str {
        match self {
            Self::Bug => "a reproduction must fail first, then the fix is judged",
            Self::Quick => "one requirement, judged like a task",
            Self::Feature => "the spec tool writes a spec; you review it",
            Self::Ask => "a read-only answer, nothing changes",
            Self::Explore => "approaches to choose from, then a brief that starts the spec",
            Self::None => "",
        }
    }

    /// Whether this build has the rung.
    #[must_use]
    pub const fn available(self) -> bool {
        matches!(
            self,
            Self::Bug | Self::Quick | Self::Feature | Self::Ask | Self::Explore
        )
    }
}

/// The rule that decided a suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RungCueRule {
    /// Words that weigh approaches ("should we", "which is better").
    Weighing,
    /// An interrogative first word or a trailing `?`.
    Question,
    /// A word for a wrong result of existing behaviour.
    WrongResult,
    /// Two or more capabilities joined.
    SeveralCapabilities,
    /// More than [`LONG_WORDS`] words.
    Long,
    /// None of the above.
    Default,
}

/// What decided a suggestion: the rule and the words of the text that met
/// it, verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RungCue {
    /// The rule.
    pub rule: RungCueRule,
    /// The words, as written.
    pub matched: Vec<String>,
}

/// The composer's suggestion for a text (`rung_suggestion`): computed by
/// code from the text alone, shown with its reason, never applied without
/// the person's key or click.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RungSuggestion {
    /// The suggested rung.
    pub rung: RungPick,
    /// Whether this build has it.
    pub available: bool,
    /// What decided it.
    pub cue: RungCue,
    /// Why, in one sentence.
    pub reason: String,
    /// The other rungs, in the composer's order.
    pub alternatives: Vec<RungPick>,
    /// The command that takes the suggested rung (none for `none`).
    pub command: Option<String>,
}

/// More words than this suggest a feature.
pub const LONG_WORDS: usize = 25;

/// Interrogatives that make a text a question when it starts with one.
/// `when` and `where` start bug reports as often as questions, so only a
/// trailing `?` makes those a question.
const INTERROGATIVES: &[&str] = &[
    "why", "how", "what", "which", "who", "whom", "whose", "does", "do", "did", "is", "are", "can",
    "could", "should", "would", "will", "was", "were",
];

/// Words and phrases for a wrong result of existing behaviour.
/// Words that weigh approaches rather than name one: an idea to explore.
const WEIGHING: &[&str] = &[
    "should we",
    "should i",
    "which is better",
    "how could we",
    "how should we",
    "options for",
    "compare",
    "or should",
    "best way to",
    "trade-offs",
    "tradeoffs",
];

const WRONG_RESULT: &[&str] = &[
    "crash",
    "crashes",
    "crashed",
    "crashing",
    "broken",
    "breaks",
    "wrong",
    "incorrect",
    "regression",
    "regressed",
    "error",
    "errors",
    "fails",
    "failing",
    "failed",
    "fails when",
    "panic",
    "panics",
    "hangs",
    "freezes",
    "exception",
    "is empty",
    "are empty",
    "comes back empty",
    "is missing",
    "are missing",
    "doesn't",
    "does not",
    "don't",
    "do not",
    "didn't",
    "won't",
    "can't",
    "cannot",
    "no longer",
    "instead of",
    "not working",
    "bug",
];

/// Words in `text` with their original spelling: runs of letters, digits,
/// apostrophes and hyphens.
fn words_of(text: &str) -> Vec<&str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '’' || c == '-'))
        .map(|w| w.trim_matches(|c| c == '\'' || c == '’' || c == '-'))
        .filter(|w| !w.is_empty())
        .collect()
}

fn lower(w: &str) -> String {
    w.to_lowercase().replace('’', "'")
}

/// The phrases of `lexicon` the words contain, each as written, in text order.
fn phrases(words: &[&str], lexicon: &[&str]) -> Vec<String> {
    let lowered: Vec<String> = words.iter().map(|w| lower(w)).collect();
    let mut out: Vec<(usize, String)> = Vec::new();
    for phrase in lexicon {
        let parts: Vec<&str> = phrase.split(' ').collect();
        if parts.len() > lowered.len() {
            continue;
        }
        for i in 0..=lowered.len() - parts.len() {
            if parts.iter().enumerate().all(|(j, p)| lowered[i + j] == *p) {
                out.push((i, words[i..i + parts.len()].join(" ")));
            }
        }
    }
    out.sort();
    // A longer phrase covers the shorter one at the same place ("fails when").
    let mut kept: Vec<(usize, String)> = Vec::new();
    for (i, p) in out {
        if let Some(last) = kept.last_mut()
            && last.0 == i
        {
            if p.len() > last.1.len() {
                *last = (i, p);
            }
            continue;
        }
        kept.push((i, p));
    }
    let mut seen = std::collections::BTreeSet::new();
    kept.into_iter()
        .map(|(_, p)| p)
        .filter(|p| seen.insert(lower(p)))
        .collect()
}

/// The joiners of two or more capability clauses: `and`, `;`, `,` or list
/// bullets between clauses of two words or more.
fn joiners(text: &str) -> Vec<String> {
    let mut clauses: Vec<String> = vec![String::new()];
    let mut joins: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        let bullet = t.starts_with("- ")
            || t.starts_with("* ")
            || t.split_once(". ").is_some_and(|(n, _)| {
                !n.is_empty() && n.len() <= 2 && n.chars().all(|c| c.is_ascii_digit())
            });
        if bullet && clauses.last().is_some_and(|c| !c.trim().is_empty()) {
            clauses.push(String::new());
            joins.push("list".to_owned());
        }
        let mut word = String::new();
        let flush = |word: &mut String, clauses: &mut Vec<String>, joins: &mut Vec<String>| {
            if word.eq_ignore_ascii_case("and") {
                clauses.push(String::new());
                joins.push(std::mem::take(word));
            } else if !word.is_empty() {
                if let Some(c) = clauses.last_mut() {
                    c.push(' ');
                    c.push_str(word);
                }
                word.clear();
            }
        };
        for ch in t.chars() {
            match ch {
                ';' | ',' => {
                    flush(&mut word, &mut clauses, &mut joins);
                    clauses.push(String::new());
                    joins.push(ch.to_string());
                }
                c if c.is_whitespace() => flush(&mut word, &mut clauses, &mut joins),
                c => word.push(c),
            }
        }
        flush(&mut word, &mut clauses, &mut joins);
    }
    // A clause is a capability when it has two words or more.
    let real: Vec<bool> = clauses
        .iter()
        .map(|c| c.split_whitespace().count() >= 2)
        .collect();
    if real.iter().filter(|r| **r).count() < 2 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (i, j) in joins.into_iter().enumerate() {
        // The joiner sits between clause i and clause i + 1.
        if real.get(i).copied().unwrap_or(false) && real.get(i + 1).copied().unwrap_or(false) {
            out.push(j);
        }
    }
    out
}

/// The text as one argument of a command line: in double quotes, with
/// what a shell would still expand there (`\`, `"`, `$`, `` ` ``)
/// escaped — model-written words pasted into a shell stay words.
#[must_use]
pub fn quoted(text: &str) -> String {
    let one: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::with_capacity(one.len() + 2);
    out.push('"');
    for c in one.chars() {
        if matches!(c, '\\' | '"' | '$' | '`') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// The command that takes `rung` for `text`, with the format's creating
/// skill for a feature (`specify`, `propose`).
#[must_use]
pub fn command_for(rung: RungPick, text: &str, create_skill: Option<&str>) -> Option<String> {
    let q = quoted(text);
    match rung {
        RungPick::Bug => Some(format!("speccoder bug {q}")),
        RungPick::Quick => Some(format!("speccoder do {q}")),
        RungPick::Feature => create_skill.map(|s| format!("speccoder spec {s} {q}")),
        RungPick::Ask => Some(format!("speccoder ask {q}")),
        RungPick::Explore => Some(format!("speccoder spec explore {q}")),
        RungPick::None => None,
    }
}

/// The rung a text suggests, by ordered rules (the first that holds
/// decides): a question → Ask; a wrong result → Bug fix; two or more
/// capabilities joined, or more than [`LONG_WORDS`] words → Feature; else
/// Quick change. A pure function of the text; no model takes part. The
/// `command` is left for the caller, who knows the format
/// ([`command_for`]).
#[must_use]
pub fn suggest(text: &str) -> RungSuggestion {
    let trimmed = text.trim();
    let words = words_of(trimmed);
    let wrong = phrases(&words, WRONG_RESULT);
    let first = words.first().map(|w| lower(w));
    let asks = trimmed.ends_with('?')
        || (wrong.is_empty()
            && first
                .as_deref()
                .is_some_and(|f| INTERROGATIVES.contains(&f)));
    let joined = joiners(trimmed);
    let weighing = phrases(&words, WEIGHING);
    let (rung, rule, matched) = if words.is_empty() {
        (RungPick::None, RungCueRule::Default, Vec::new())
    } else if !weighing.is_empty() {
        (RungPick::Explore, RungCueRule::Weighing, weighing)
    } else if asks {
        let m = if trimmed.ends_with('?') {
            vec!["?".to_owned()]
        } else {
            words
                .first()
                .map(|w| vec![(*w).to_owned()])
                .unwrap_or_default()
        };
        (RungPick::Ask, RungCueRule::Question, m)
    } else if !wrong.is_empty() {
        (RungPick::Bug, RungCueRule::WrongResult, wrong)
    } else if !joined.is_empty() {
        (RungPick::Feature, RungCueRule::SeveralCapabilities, joined)
    } else if words.len() > LONG_WORDS {
        (RungPick::Feature, RungCueRule::Long, Vec::new())
    } else {
        (RungPick::Quick, RungCueRule::Default, Vec::new())
    };
    let shown = |m: &[String]| {
        m.iter()
            .map(|w| format!("\"{w}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let reason = match rule {
        RungCueRule::Weighing => format!(
            "Why \"Explore\": it weighs approaches ({}) — explore them, choose one, then the spec.",
            shown(&matched)
        ),
        RungCueRule::Question => "Why \"Just ask\": it reads like a question — Ask answers it from what it reads and changes nothing.".to_owned(),
        RungCueRule::WrongResult => format!(
            "Why \"Bug fix\": the text names a wrong result ({}) of existing behaviour.",
            shown(&matched)
        ),
        RungCueRule::SeveralCapabilities => {
            "Why \"Feature\": the text joins two or more capabilities.".to_owned()
        }
        RungCueRule::Long => format!(
            "Why \"Feature\": {} words is more than one requirement holds.",
            words.len()
        ),
        RungCueRule::Default if rung == RungPick::None => "Describe the change first.".to_owned(),
        RungCueRule::Default => {
            "Why \"Quick change\": one change, no wrong result named and no list of capabilities."
                .to_owned()
        }
    };
    RungSuggestion {
        rung,
        available: rung.available(),
        cue: RungCue { rule, matched },
        reason,
        alternatives: RungPick::ORDER.into_iter().filter(|p| *p != rung).collect(),
        command: None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_quoted_argument_expands_nothing_in_a_shell() {
        assert_eq!(quoted("log  every\nsign-in"), "\"log every sign-in\"");
        assert_eq!(
            quoted("run $(curl x|sh) and `id` \"now\" \\"),
            "\"run \\$(curl x|sh) and \\`id\\` \\\"now\\\" \\\\\""
        );
    }

    use super::*;

    #[test]
    fn origins_carry_their_kind_and_read_back() {
        let o = Origin::Issue {
            number: 42,
            repo: Some("o/r".to_owned()),
            context_sha256: None,
        };
        let v = serde_json::to_value(&o).unwrap_or_default();
        assert_eq!(v["kind"], "issue");
        assert_eq!(serde_json::from_value::<Origin>(v).ok(), Some(o.clone()));
        assert_eq!(o.describe(), "issue o/r#42");
        assert_eq!(subject("007-bugfixes", "FR-003"), "007-bugfixes/FR-003");
    }

    #[test]
    fn every_rung_is_decided_by_the_standing_units() {
        let standing = vec![
            ("004-quick-changes".to_owned(), Rung::Quick),
            ("007-bugfixes".to_owned(), Rung::Bug),
            (".specify/bugs".to_owned(), Rung::Bug),
        ];
        assert_eq!(rung_of("004-quick-changes", &standing), Rung::Quick);
        assert_eq!(rung_of("007-bugfixes", &standing), Rung::Bug);
        assert_eq!(rung_of(".specify/bugs/empty-title", &standing), Rung::Bug);
        assert_eq!(rung_of("007-bugfixes-extra", &standing), Rung::Feature);
        assert_eq!(rung_of("001-greet", &standing), Rung::Feature);
        assert_eq!(rung_of("001-greet", &[]), Rung::Feature);
        assert_eq!(Rung::Quick.as_str(), "quick");
    }

    fn input(files: u64) -> SignalInput {
        SignalInput {
            files_changed: files,
            promote_files: 8,
            ..SignalInput::default()
        }
    }

    #[test]
    fn signals_hold_on_recorded_facts_only() {
        assert!(signals(&SignalInput::default()).is_empty());
        assert!(signals(&input(8)).is_empty());
        let s = signals(&input(11));
        assert_eq!(
            (s[0].id.as_str(), s[0].value, s[0].threshold),
            ("S1", 11, 8)
        );
        // Monotone in files: past the threshold it keeps holding.
        for n in 9..40 {
            assert_eq!(signals(&input(n)).len(), 1);
        }
        let two = SignalInput {
            red_causes: vec!["a test failed".into(), "a test failed".into()],
            ..input(1)
        };
        assert!(signals(&two).is_empty(), "the same cause twice is one");
        let distinct = SignalInput {
            red_causes: vec!["a test failed".into(), "the build broke".into()],
            ambiguity_markers: 1,
            judge_paths: vec!["speccoder.toml".into()],
            packages_changed: 2,
            ..input(3)
        };
        let ids: Vec<String> = signals(&distinct).into_iter().map(|s| s.id).collect();
        assert_eq!(ids, ["S1", "S2", "S3", "S4"]);
        assert_eq!(signals(&distinct), signals(&distinct.clone()));
    }
}
