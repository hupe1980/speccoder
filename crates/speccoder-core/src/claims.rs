//! Story claims: who works on which story, kept as refs on the shared
//! remote (`refs/speccoder/claims/<feature>/<story>`). A claim is advisory —
//! git does not enforce it; speccoder's run and `pr` respect it, and a
//! person can always take it over. It never locks files.

use serde::{Deserialize, Serialize};

/// How long a claim lasts after its last renewal.
pub const WINDOW_S: u64 = 24 * 60 * 60;
/// A done claim holds at most this long: a branch merged by squash or
/// rebase leaves no ancestry to see the merge by.
pub const DONE_WINDOW_S: u64 = 14 * 24 * 60 * 60;
/// A run renews its claim once the last renewal is older than this.
pub const RENEW_AFTER_S: u64 = 60 * 60;
/// Where claims live.
pub const NAMESPACE: &str = "refs/speccoder/claims";

/// A claim's ref.
#[must_use]
pub fn ref_of(feature: &str, story: &str) -> String {
    format!("{NAMESPACE}/{feature}/{story}")
}

/// One story's claim, as its commit message records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Claim {
    /// The feature (its directory name).
    pub feature: String,
    /// The story (`US2`).
    pub story: String,
    /// Who holds it: git `user.name`.
    pub holder: String,
    /// Their git `user.email`: the identity, compared without case.
    pub email: String,
    /// First claimed (Unix seconds); kept on renewal.
    pub since: u64,
    /// Lasts until (Unix seconds).
    pub until: u64,
    /// The branch the work is on.
    pub branch: String,
    /// The commit the story's last task was ticked at, once done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done: Option<String>,
}

impl Claim {
    /// The claim commit's message.
    #[must_use]
    pub fn message(&self) -> String {
        let mut m = format!(
            "Claim {} of {}\n\nClaimed-By: {} <{}>\nClaimed-Since: {}\nClaimed-Until: {}\nClaimed-Branch: {}\n",
            self.story, self.feature, self.holder, self.email, self.since, self.until, self.branch
        );
        if let Some(d) = &self.done {
            m.push_str(&format!("Claimed-Done: {d}\n"));
        }
        m
    }

    /// Reads a claim commit's message; `None` when it is not one.
    #[must_use]
    pub fn parse(feature: &str, story: &str, message: &str) -> Option<Self> {
        // What another person's push says is shown in terminals: no
        // control characters.
        let field = |k: &str| {
            message.lines().find_map(|l| {
                l.strip_prefix(k)
                    .and_then(|r| r.strip_prefix(':'))
                    .map(|v| {
                        v.chars()
                            .filter(|c| !c.is_control())
                            .collect::<String>()
                            .trim()
                            .to_owned()
                    })
            })
        };
        let by = field("Claimed-By")?;
        let (holder, email) = match by.rsplit_once('<') {
            Some((n, e)) => (
                n.trim().to_owned(),
                e.trim_end_matches('>').trim().to_owned(),
            ),
            None => (by.clone(), String::new()),
        };
        if email.is_empty() {
            return None;
        }
        Some(Self {
            feature: feature.to_owned(),
            story: story.to_owned(),
            holder,
            email,
            since: field("Claimed-Since")?.parse().ok()?,
            until: field("Claimed-Until")?.parse().ok()?,
            branch: field("Claimed-Branch").unwrap_or_default(),
            done: field("Claimed-Done").filter(|d| !d.is_empty()),
        })
    }

    /// Whether `email` holds it.
    #[must_use]
    pub fn is_of(&self, email: &str) -> bool {
        self.email.eq_ignore_ascii_case(email.trim())
    }

    /// Its state for `me` at `now`.
    #[must_use]
    pub fn state(&self, me: Option<&str>, now: u64) -> ClaimState {
        if me.is_some_and(|m| self.is_of(m)) {
            ClaimState::Mine
        } else if self.until <= now {
            ClaimState::Stale
        } else if self.done.is_some() {
            ClaimState::Done
        } else {
            ClaimState::Theirs
        }
    }
}

/// A claim as one person sees it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ClaimState {
    /// Another person's, live: their run or their hands are on it.
    Theirs,
    /// Another person's, done on their branch and not merged yet.
    Done,
    /// The person's own (any machine).
    Mine,
    /// Another person's, past its time: anyone may take it.
    Stale,
    /// The ref's message is not a claim: anyone may take it.
    Unreadable,
}

impl ClaimState {
    /// Another person's work stands in the way.
    #[must_use]
    pub const fn blocks(self) -> bool {
        matches!(self, Self::Theirs | Self::Done)
    }
}

/// One listed claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClaimView {
    /// The story.
    pub story: String,
    /// Its state for the person.
    pub state: ClaimState,
    /// The claim; none when unreadable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim: Option<Claim>,
}

/// What a `claim` changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClaimVerb {
    /// A free story is now the person's.
    Claimed,
    /// The person's own claim lasts longer.
    Renewed,
    /// Another person's claim is now the person's.
    Took,
    /// The person's own claim is gone.
    Released,
    /// Another person's claim is gone.
    Removed,
}

/// One change, with whose claim it replaced or removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClaimChange {
    /// The story.
    pub story: String,
    /// What happened.
    pub verb: ClaimVerb,
    /// Whose claim it was, when another's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// That claim was past its time.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stale: bool,
}

/// `claim`: a feature's claims, and what this call changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClaimsReport {
    /// The feature.
    pub feature: String,
    /// Where claims live: `origin` (a remote's name), or `None` — this
    /// repository only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    /// Read from the remote just now (`false`: the local copy as last fetched).
    pub live: bool,
    /// The person's git email, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub me: Option<String>,
    /// The claims, by story.
    pub claims: Vec<ClaimView>,
    /// What this call changed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed: Vec<ClaimChange>,
    /// Said once: the fetch refspec added, a merged claim removed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<String>,
    /// When it was read (Unix seconds).
    pub at: u64,
}

impl ClaimsReport {
    /// Other people's claims that stand in the way.
    pub fn others(&self) -> impl Iterator<Item = &Claim> {
        self.claims
            .iter()
            .filter(|c| c.state.blocks())
            .filter_map(|c| c.claim.as_ref())
    }
}

/// `in 3h`, `5m ago`: a time relative to `now`, for one line.
#[must_use]
pub fn relative(t: u64, now: u64) -> String {
    let (d, past) = if t >= now {
        (t - now, false)
    } else {
        (now - t, true)
    };
    let span = match d {
        0..60 => return "just now".to_owned(),
        60..3600 => format!("{}m", (d + 30) / 60),
        3600..172_800 => format!("{}h", (d + 1800) / 3600),
        _ => format!("{}d", (d + 43_200) / 86_400),
    };
    if past {
        format!("{span} ago")
    } else {
        format!("in {span}")
    }
}

/// One line for a claim another person holds: *US2 is Dana's (since 2h ago,
/// until in 22h)* or *US2 is done on Dana's speccoder/004-export, not merged*.
#[must_use]
pub fn held_line(c: &Claim, now: u64) -> String {
    match &c.done {
        Some(_) => format!(
            "{} is done on {}'s {}, not merged yet",
            c.story, c.holder, c.branch
        ),
        None => format!(
            "{} is claimed by {} (since {}, until {})",
            c.story,
            c.holder,
            relative(c.since, now),
            relative(c.until, now)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dana() -> Claim {
        Claim {
            feature: "004-export".into(),
            story: "US2".into(),
            holder: "Dana Doe".into(),
            email: "dana@example.com".into(),
            since: 1000,
            until: 1000 + WINDOW_S,
            branch: "speccoder/004-export".into(),
            done: None,
        }
    }

    #[test]
    fn the_message_reads_back() {
        let c = dana();
        assert_eq!(
            Claim::parse("004-export", "US2", &c.message()),
            Some(c.clone())
        );
        let done = Claim {
            done: Some("abc123".into()),
            ..c
        };
        assert_eq!(
            Claim::parse("004-export", "US2", &done.message()),
            Some(done)
        );
        assert_eq!(Claim::parse("f", "US1", "a commit\n"), None);
        assert_eq!(
            Claim::parse(
                "f",
                "US1",
                "Claimed-By: x\nClaimed-Since: 1\nClaimed-Until: 2\n"
            ),
            None
        );
    }

    #[test]
    fn states() {
        let c = dana();
        assert_eq!(c.state(Some("DANA@example.com "), 5), ClaimState::Mine);
        assert_eq!(c.state(Some("sam@x"), 5), ClaimState::Theirs);
        assert_eq!(c.state(None, c.until), ClaimState::Stale);
        let done = Claim {
            done: Some("abc".into()),
            ..c
        };
        assert_eq!(done.state(Some("sam@x"), 5), ClaimState::Done);
        // A done claim past its window: merged by squash, or left.
        assert_eq!(done.state(Some("sam@x"), u64::MAX), ClaimState::Stale);
        let shouting = Claim::parse(
            "f",
            "US1",
            "Claimed-By: Eve\u{1b}[31m <e@x>\nClaimed-Since: 1\nClaimed-Until: 2\n",
        )
        .unwrap();
        assert_eq!(shouting.holder, "Eve[31m");
        assert!(ClaimState::Done.blocks() && !ClaimState::Stale.blocks());
    }

    #[test]
    fn lines() {
        assert_eq!(relative(100, 100), "just now");
        assert_eq!(relative(100 + 7200, 100), "in 2h");
        assert_eq!(relative(100, 100 + 300), "5m ago");
        let c = dana();
        assert_eq!(
            held_line(&c, 1000 + 7200),
            "US2 is claimed by Dana Doe (since 2h ago, until in 22h)"
        );
    }
}
