//! The command surface as a person meets it (F-057): the daily commands
//! grouped by what they are for, the rest behind `help advanced`, and
//! shell completion. The descriptions are the commands' own (clap's
//! `about`), so the help cannot drift from the commands.

use clap::CommandFactory;

use crate::cli::Cli;

/// The daily commands, by group, in the order shown.
pub const GROUPS: &[(&str, &[&str])] = &[
    ("Start", &["init", "doctor", "config"]),
    ("Plan", &["feature", "spec"]),
    ("Work", &["run", "task"]),
    ("Review", &["report", "ui", "pr", "attest"]),
];

/// The commands `help advanced` lists.
pub const ADVANCED: &[&str] = &[
    "verify",
    "converge",
    "inspect",
    "replay",
    "worktree",
    "acp",
    "workflow",
    "eval",
    "bench",
    "propose",
    "explain",
    "schema",
    "completion",
    "tui",
];

fn about(cmd: &clap::Command, name: &str) -> String {
    cmd.find_subcommand(name)
        .and_then(|c| c.get_about())
        .map(ToString::to_string)
        .unwrap_or_default()
}

fn listing(cmd: &clap::Command, names: &[&str]) -> String {
    names
        .iter()
        .map(|n| format!("  {n:<11} {}\n", about(cmd, n)))
        .collect()
}

/// The top-level help template: groups, then the options, then where the
/// rest is.
#[must_use]
pub fn template() -> String {
    let cmd = Cli::command();
    let mut groups = String::new();
    for (title, names) in GROUPS {
        groups.push_str(&format!("{title}\n{}", listing(&cmd, names)));
    }
    format!(
        "{{about-with-newline}}\n{{usage-heading}} {{usage}}\n\n{groups}\nOptions:\n{{options}}\n\nMore: speccoder help advanced · speccoder help <command> · bare `speccoder`: the dashboard\n"
    )
}

/// The CLI with the grouped help. `--help` at the top is the short form
/// (one line per option, values inline) so it fits a screen.
#[must_use]
pub fn command() -> clap::Command {
    Cli::command().help_template(template()).mut_args(|a| {
        let takes_value = a.get_action().takes_values();
        let a = a.long_help(None);
        if takes_value {
            a.hide_possible_values(true)
        } else {
            a
        }
    })
}

/// `speccoder help [advanced | COMMAND…]`.
pub fn print(topic: &[String]) {
    let mut cmd = command();
    match topic.first().map(String::as_str) {
        None => {
            let _ = cmd.print_help();
        }
        Some("advanced") => {
            print!(
                "Advanced commands (`speccoder help <command>` for each):\n\n{}",
                listing(&cmd, ADVANCED)
            );
        }
        Some(_) => {
            let mut at = &mut cmd;
            for name in topic {
                match at.find_subcommand_mut(name) {
                    Some(sub) => at = sub,
                    None => {
                        eprintln!(
                            "speccoder: no command `{}`; `speccoder help` lists them",
                            topic.join(" ")
                        );
                        std::process::exit(2);
                    }
                }
            }
            let _ = at.print_long_help();
        }
    }
}

/// `speccoder completion SHELL`.
pub fn completion(shell: clap_complete::Shell) {
    let mut cmd = command();
    clap_complete::generate(shell, &mut cmd, "speccoder", &mut std::io::stdout());
}

/// Former commands and what replaced them (hard cut: no aliases).
pub const FORMER: &[(&str, &str)] = &[
    ("approve", "speccoder task approve <TASK>"),
    ("reject", "speccoder task reject <TASK> --reason \"…\""),
    ("accept", "speccoder task accept <TASK> --reason \"…\""),
    ("skip", "speccoder task skip <TASK> --reason \"…\""),
    ("nudge", "speccoder task nudge <TASK> --note \"…\""),
    ("approvals", "speccoder report --approvals"),
    ("status", "speccoder report --live"),
    ("cost", "speccoder report --cost"),
    ("skill", "speccoder spec <SKILL>"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_top_level_help_is_short_and_grouped() {
        let help = command().render_help().to_string();
        let lines = help.lines().count();
        assert!(lines <= 40, "{lines} lines:\n{help}");
        for (title, names) in GROUPS {
            assert!(help.contains(&format!("{title}\n")), "{help}");
            for n in *names {
                assert!(help.contains(&format!("  {n} ")), "{n}: {help}");
            }
        }
        for n in ADVANCED {
            assert!(
                !help.contains(&format!("  {n:<11} ")),
                "{n} is advanced: {help}"
            );
        }
        assert!(help.contains("speccoder help advanced"));
    }

    #[test]
    fn every_command_is_daily_or_advanced_and_described() {
        let cmd = command();
        let daily: Vec<&str> = GROUPS.iter().flat_map(|(_, n)| n.iter().copied()).collect();
        for sub in cmd.get_subcommands() {
            let name = sub.get_name();
            if name == "help" {
                continue;
            }
            assert!(
                daily.contains(&name) != ADVANCED.contains(&name),
                "{name} must be in exactly one list"
            );
            assert_eq!(sub.is_hide_set(), ADVANCED.contains(&name), "{name}");
            assert!(sub.get_about().is_some(), "{name} has no description");
        }
    }

    #[test]
    fn completion_scripts_name_the_task_verbs() {
        let mut out = Vec::new();
        clap_complete::generate(
            clap_complete::Shell::Bash,
            &mut command(),
            "speccoder",
            &mut out,
        );
        let script = String::from_utf8(out).unwrap();
        assert!(
            script.contains("approve") && script.contains("doctor"),
            "{}",
            &script[..200]
        );
    }
}
