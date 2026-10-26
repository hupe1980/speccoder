//! `speccoder actions` for a person: where the feature is, then what can
//! be done now, group by group — what each does, why, the command it
//! stands for, and what stands in its way.

use speccoder_core::ActionsReport;

use crate::style::{Look, Token};

/// The groups, in the order the list is read.
const GROUPS: &[(&str, &str)] = &[
    ("next", "Next"),
    ("plan", "Plan"),
    ("work", "Work"),
    ("task", "Task"),
    ("feature", "Feature"),
    ("review", "Review"),
];

/// The lines of `r` in `look`.
#[must_use]
pub fn lines(r: &ActionsReport, look: &Look) -> Vec<String> {
    let mut out = vec![crate::human::journey_in(look, &r.journey)];
    // Groups the list has that this build does not know are shown too.
    let mut groups: Vec<(&str, &str)> = GROUPS.to_vec();
    for a in &r.actions {
        if !groups.iter().any(|(g, _)| *g == a.group) {
            groups.push((a.group.as_str(), a.group.as_str()));
        }
    }
    for (group, title) in groups {
        let actions: Vec<_> = r.actions.iter().filter(|a| a.group == group).collect();
        if actions.is_empty() {
            continue;
        }
        out.push(String::new());
        out.push(look.paint(Token::Accent, title));
        for a in actions {
            if look.screen_reader {
                let mut line = format!("{}: {}. Command: {}.", a.label, a.why, a.command);
                if let Some(b) = &a.blocked {
                    line.push_str(&format!(" Blocked: {b}."));
                }
                if let Some(i) = &a.input {
                    line.push_str(&format!(" Asks: {i}."));
                }
                if a.confirm.is_some() {
                    line.push_str(" Confirms first.");
                }
                out.push(line);
                continue;
            }
            out.push(format!(
                "  {}  {}",
                a.label,
                look.paint(Token::Muted, &a.why)
            ));
            out.push(format!(
                "    {} {}",
                look.paint(Token::Muted, "$"),
                a.command
            ));
            if let Some(b) = &a.blocked {
                out.push(format!(
                    "    {}",
                    look.paint(Token::Attention, &format!("blocked: {b}"))
                ));
            }
            if let Some(i) = &a.input {
                out.push(format!(
                    "    {}",
                    look.paint(Token::Muted, &format!("asks: {i}"))
                ));
            }
            if a.confirm.is_some() {
                out.push(format!(
                    "    {}",
                    look.paint(Token::Muted, "confirms first")
                ));
            }
        }
    }
    if look.ascii {
        out.into_iter().map(|l| crate::style::asciify(&l)).collect()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use speccoder_core::actions::{Action, Runnable};

    use super::*;

    fn report() -> ActionsReport {
        let mut run = Action::new(
            "next",
            "run the feature",
            "2 ready task(s)",
            "speccoder run",
        )
        .with_run(Runnable::Run { task: None });
        run.blocked = Some("commit the planning files first".to_owned());
        let mut pr = Action::new(
            "review",
            "open a draft pull request",
            "push the branch",
            "speccoder pr --yes",
        )
        .with_run(Runnable::PullRequest);
        pr.confirm = Some("Push it?".to_owned());
        ActionsReport {
            actions: vec![run, pr],
            journey: speccoder_core::actions::journey(speccoder_core::actions::JourneyFacts {
                spec: true,
                plan: true,
                tasks: true,
                uncommitted: true,
                total: 2,
                finished: 0,
            }),
        }
    }

    #[test]
    fn the_list_reads_group_by_group_with_each_command() {
        let text = lines(&report(), &Look::PLAIN).join("\n");
        for needle in [
            "● commit",
            "Next\n  run the feature  2 ready task(s)\n    $ speccoder run\n    blocked: commit the planning files first",
            "Review\n  open a draft pull request  push the branch\n    $ speccoder pr --yes\n    confirms first",
        ] {
            assert!(text.contains(needle), "missing {needle:?} in\n{text}");
        }
        let ascii = Look {
            ascii: true,
            ..Look::PLAIN
        };
        assert!(lines(&report(), &ascii).join("\n").is_ascii());
        let reader = Look {
            screen_reader: true,
            ..Look::PLAIN
        };
        let text = lines(&report(), &reader).join("\n");
        assert!(
            text.contains("run the feature: 2 ready task(s). Command: speccoder run. Blocked: commit the planning files first."),
            "{text}"
        );
    }
}
