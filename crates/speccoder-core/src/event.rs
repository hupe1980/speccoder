//! The names of `--events` lines: one closed list. A line whose type is
//! not here is never emitted; a new name is lowercase `noun.verb`. Domain
//! events (a record backs them) publish a projection schema; progress
//! events are stream-only.

/// What kind of event a name is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// A record backs it; its public projection is published.
    Domain,
    /// Stream-only.
    Progress,
}

/// One registered name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Name {
    /// The `type` of the line.
    pub name: &'static str,
    /// Domain or progress.
    pub class: Class,
}

const fn p(name: &'static str) -> Name {
    Name {
        name,
        class: Class::Progress,
    }
}

const fn d(name: &'static str) -> Name {
    Name {
        name,
        class: Class::Domain,
    }
}

/// Names that predate the rule (single words, an underscore); kept.
pub const GRANDFATHERED: &[&str] = &[
    "tool_call",
    "usage",
    "said",
    "reasoning",
    "notice",
    "result",
];

/// Every name a stream may carry.
pub const REGISTRY: &[Name] = &[
    p("session.started"),
    p("run.started"),
    p("task.started"),
    p("stage.started"),
    p("stage.finished"),
    p("task.resumed"),
    p("tool_call"),
    p("approval.requested"),
    p("approval.decided"),
    p("usage"),
    p("task.finished"),
    p("agent.started"),
    p("agent.plan"),
    p("agent.unasked"),
    p("said"),
    p("reasoning"),
    p("notice"),
    p("result"),
    d("bug.recorded"),
    p("bug.reproduce_try"),
    p("bug.reproduced"),
    p("bug.confirm_waiting"),
    d("bug.confirmed"),
    d("bug.confirm_rejected"),
    p("bug.unchanged_selected"),
    p("bug.discrimination"),
    d("bug.outcome"),
    p("oracle.stage"),
    d("oracle.frozen"),
    d("oracle.lapsed"),
    d("collection.checked"),
    p("regression.bisected"),
    d("project.created"),
    d("project.undone"),
    p("scaffold.started"),
    p("scaffold.finished"),
    p("planning.stopped"),
];

/// The registered name `name`.
#[must_use]
pub fn lookup(name: &str) -> Option<&'static Name> {
    REGISTRY.iter().find(|n| n.name == name)
}

/// A new name's rule: lowercase words joined by one dot
/// (`[a-z_]+\.[a-z_]+`).
#[must_use]
pub fn well_formed(name: &str) -> bool {
    let word = |w: &str| !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c == '_');
    match name.split_once('.') {
        Some((a, b)) => word(a) && word(b),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_is_registered_once_and_new_ones_follow_the_rule() {
        let mut seen = std::collections::BTreeSet::new();
        for n in REGISTRY {
            assert!(seen.insert(n.name), "{} twice", n.name);
            assert!(
                well_formed(n.name) || GRANDFATHERED.contains(&n.name),
                "{} breaks the name rule",
                n.name
            );
        }
        for g in GRANDFATHERED {
            assert!(lookup(g).is_some(), "{g}");
        }
        assert!(well_formed("bug.reproduce_try"));
        assert!(!well_formed("bug_outcome"));
        assert!(!well_formed("Bug.Outcome"));
        assert!(!well_formed("bug.out.come"));
        assert_eq!(lookup("bug.outcome").map(|n| n.class), Some(Class::Domain));
        assert!(lookup("bug.unknown").is_none());
    }
}
