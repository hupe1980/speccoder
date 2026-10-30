//! The person's own settings, `~/.config/speccoder/config.toml`:
//! how speccoder gets their attention. Never read from the committed
//! `speccoder.toml`, which is the project's policy.

use std::path::PathBuf;
use std::time::Duration;

/// What may ask for attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// A call waits for a person.
    Approval,
    /// A task failed or stopped.
    TaskFailed,
    /// A feature run ended.
    FeatureDone,
    /// Nothing happened for `stall_after`.
    Stalled,
}

impl Signal {
    const ALL: [Self; 4] = [
        Self::Approval,
        Self::TaskFailed,
        Self::FeatureDone,
        Self::Stalled,
    ];

    fn key(self) -> &'static str {
        match self {
            Self::Approval => "approval",
            Self::TaskFailed => "task_failed",
            Self::FeatureDone => "feature_done",
            Self::Stalled => "stalled",
        }
    }
}

/// How attention is asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    /// OSC 9 or OSC 777 where the terminal shows them, else BEL.
    Auto,
    /// `ESC ] 9 ; message BEL`.
    Osc9,
    /// `ESC ] 777 ; notify ; speccoder ; message BEL`.
    Osc777,
    /// BEL.
    Bel,
    /// A program; the message is its last argument.
    Command(Vec<String>),
    /// Nothing.
    Off,
}

/// `[ui.notify]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notify {
    /// The signals that notify.
    pub on: Vec<Signal>,
    /// Only when the terminal is not focused (plain mode counts as unfocused).
    pub unfocused_only: bool,
    /// How.
    pub method: Method,
    /// No event for this long is a stall.
    pub stall_after: Duration,
}

impl Default for Notify {
    fn default() -> Self {
        Self {
            // Only what blocks work: a call waiting, a task that did not
            // pass. A finished feature or a stall notifies when listed.
            on: vec![Signal::Approval, Signal::TaskFailed],
            unfocused_only: true,
            method: Method::Auto,
            stall_after: Duration::from_secs(15 * 60),
        }
    }
}

impl Notify {
    /// Whether `signal` notifies.
    #[must_use]
    pub fn wants(&self, signal: Signal) -> bool {
        self.on.contains(&signal)
    }
}

/// The settings file: `$XDG_CONFIG_HOME/speccoder/config.toml` when set,
/// else `$HOME/.config/speccoder/config.toml`.
#[must_use]
pub fn path() -> Option<PathBuf> {
    let xdg = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty());
    match xdg {
        Some(dir) => Some(PathBuf::from(dir)),
        None => std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")),
    }
    .map(|base| base.join("speccoder").join("config.toml"))
}

/// How output looks (`[ui]`): values as written, checked here;
/// unset is `None` (the renderer's default).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ui {
    /// `auto`, `dark`, `light`, `high-contrast`, `daltonized`, `mono`.
    pub theme: Option<String>,
    /// `auto`, `always`, `never`.
    pub color: Option<String>,
    /// ASCII glyphs and borders.
    pub ascii: Option<bool>,
    /// Screen-reader mode.
    pub screen_reader: Option<bool>,
}

/// The person's settings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// `[ui.notify]`.
    pub notify: Notify,
    /// `[ui]`.
    pub ui: Ui,
}

/// The settings and a warning per value that was not understood (the
/// default is used for it). A missing file is all defaults.
#[must_use]
pub fn load() -> (Settings, Vec<String>) {
    let Some(p) = path() else {
        return (Settings::default(), Vec::new());
    };
    let Ok(text) = std::fs::read_to_string(&p) else {
        return (Settings::default(), Vec::new());
    };
    let file = p.display().to_string();
    let (notify, mut warnings) = parse(&text, &file);
    let (ui, more) = parse_ui(&text, &file);
    warnings.extend(more);
    warnings.extend(project_only(&text, &file));
    (Settings { notify, ui }, warnings)
}

/// Keys that are the project's policy, never the person's: a value in the
/// user config is refused with a warning naming it (until settings learn
/// which keys a person may tighten).
pub const PROJECT_ONLY: &[(&str, &str)] = &[("bug", "reproduce_tries"), ("bug", "test")];

/// One warning per [`PROJECT_ONLY`] key `text` sets.
#[must_use]
pub fn project_only(text: &str, file: &str) -> Vec<String> {
    let Ok(table) = toml::from_str::<toml::Table>(text) else {
        return Vec::new();
    };
    PROJECT_ONLY
        .iter()
        .filter(|(section, key)| table.get(*section).and_then(|s| s.get(*key)).is_some())
        .map(|(section, key)| {
            format!(
                "warning: {file}: {section}.{key}: a project setting (speccoder.toml), not read from the user config; ignored"
            )
        })
        .collect()
}

const THEMES: [&str; 6] = [
    "auto",
    "dark",
    "light",
    "high-contrast",
    "daltonized",
    "mono",
];

/// `[ui]`'s look keys from `text` (`[ui.notify]` is [`parse`]'s).
#[must_use]
pub fn parse_ui(text: &str, file: &str) -> (Ui, Vec<String>) {
    let mut ui = Ui::default();
    let mut warnings = Vec::new();
    let mut warn = |key: &str, why: &str| {
        warnings.push(format!("warning: {file}: {key}: {why}; using the default"));
    };
    let Ok(table) = toml::from_str::<toml::Table>(text) else {
        return (ui, warnings); // `parse` already said why
    };
    let Some(section) = table.get("ui").and_then(toml::Value::as_table) else {
        return (ui, warnings);
    };
    for (key, value) in section {
        match key.as_str() {
            "notify" => {}
            "theme" => match value.as_str().filter(|t| THEMES.contains(t)) {
                Some(t) => ui.theme = Some(t.to_owned()),
                None => warn(
                    "ui.theme",
                    &format!("expected one of {}", THEMES.join(", ")),
                ),
            },
            "color" => match value
                .as_str()
                .filter(|c| ["auto", "always", "never"].contains(c))
            {
                Some(c) => ui.color = Some(c.to_owned()),
                None => warn("ui.color", "expected auto, always or never"),
            },
            "ascii" => match value.as_bool() {
                Some(b) => ui.ascii = Some(b),
                None => warn("ui.ascii", "expected true or false"),
            },
            "screen_reader" => match value.as_bool() {
                Some(b) => ui.screen_reader = Some(b),
                None => warn("ui.screen_reader", "expected true or false"),
            },
            other => warn(&format!("ui.{other}"), "not a setting"),
        }
    }
    (ui, warnings)
}

/// [`load`] from `text`, naming `file` in warnings.
#[must_use]
pub fn parse(text: &str, file: &str) -> (Notify, Vec<String>) {
    let mut n = Notify::default();
    let mut warnings = Vec::new();
    let mut warn = |key: &str, why: &str| {
        warnings.push(format!("warning: {file}: {key}: {why}; using the default"));
    };
    let table: toml::Table = match toml::from_str(text) {
        Ok(t) => t,
        Err(e) => {
            warn("file", &e.to_string().replace('\n', " "));
            return (n, warnings);
        }
    };
    let Some(notify) = table
        .get("ui")
        .and_then(|u| u.get("notify"))
        .and_then(toml::Value::as_table)
    else {
        return (n, warnings);
    };
    let mut command: Vec<String> = Vec::new();
    for (key, value) in notify {
        match key.as_str() {
            "on" => match value.as_array() {
                Some(items) => {
                    n.on = items
                        .iter()
                        .filter_map(|i| {
                            let name = i.as_str().unwrap_or_default();
                            let found = Signal::ALL.into_iter().find(|s| s.key() == name);
                            if found.is_none() {
                                warn("ui.notify.on", &format!("`{name}` is not a signal"));
                            }
                            found
                        })
                        .collect();
                }
                None => warn("ui.notify.on", "expected a list"),
            },
            "when" => match value.as_str() {
                Some("unfocused") => n.unfocused_only = true,
                Some("always") => n.unfocused_only = false,
                _ => warn("ui.notify.when", "expected \"unfocused\" or \"always\""),
            },
            "method" => match value.as_str() {
                Some("auto") => n.method = Method::Auto,
                Some("osc9") => n.method = Method::Osc9,
                Some("osc777") => n.method = Method::Osc777,
                Some("bel") => n.method = Method::Bel,
                Some("off") => n.method = Method::Off,
                Some("command") => n.method = Method::Command(Vec::new()),
                _ => warn(
                    "ui.notify.method",
                    "expected auto, osc9, osc777, bel, command or off",
                ),
            },
            "command" => match value.as_array() {
                Some(items) => {
                    command = items
                        .iter()
                        .filter_map(|i| i.as_str().map(str::to_owned))
                        .collect();
                }
                None => warn("ui.notify.command", "expected a list of strings"),
            },
            "stall_after" => match value.as_str().and_then(duration) {
                Some(d) if d >= Duration::from_secs(30) => n.stall_after = d,
                _ => warn(
                    "ui.notify.stall_after",
                    "expected Ns, Nm or Nh, at least 30s",
                ),
            },
            other => warn(&format!("ui.notify.{other}"), "not a setting"),
        }
    }
    if let Method::Command(argv) = &mut n.method {
        if command.is_empty() {
            warn(
                "ui.notify.command",
                "method is \"command\" but no command is given",
            );
            n.method = Method::Off;
        } else {
            *argv = command;
        }
    }
    (n, warnings)
}

fn duration(text: &str) -> Option<Duration> {
    // The unit is the last character, which may be more than a byte.
    let (number, unit) = text.split_at(text.char_indices().last()?.0);
    let n: u64 = number.parse().ok()?;
    match unit {
        "s" => Some(Duration::from_secs(n)),
        "m" => Some(Duration::from_secs(n * 60)),
        "h" => Some(Duration::from_secs(n * 3600)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_project_setting_in_the_user_config_is_refused() {
        let w = super::project_only("[bug]\nreproduce_tries = 5\n", "config.toml");
        assert_eq!(w.len(), 1);
        assert!(
            w[0].contains("bug.reproduce_tries") && w[0].contains("ignored"),
            "{w:?}"
        );
        assert!(super::project_only("[ui]\ntheme = \"dark\"\n", "c").is_empty());
    }

    use super::*;

    #[test]
    fn a_multibyte_unit_is_no_duration() {
        for bad in ["15д", "€", "д"] {
            assert_eq!(duration(bad), None, "{bad}");
        }
        assert_eq!(duration("15m"), Some(Duration::from_secs(900)));
    }

    #[test]
    fn the_look_is_read_and_bad_values_warn() {
        let (ui, w) = parse_ui(
            "[ui]\ntheme = \"daltonized\"\ncolor = \"never\"\nascii = true\nscreen_reader = true\n",
            "c",
        );
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(ui.theme.as_deref(), Some("daltonized"));
        assert_eq!(ui.color.as_deref(), Some("never"));
        assert_eq!((ui.ascii, ui.screen_reader), (Some(true), Some(true)));
        let (ui, w) = parse_ui(
            "[ui]\ntheme = \"neon\"\nascii = \"yes\"\nsparkles = 1\n",
            "c",
        );
        assert_eq!(ui, Ui::default());
        assert_eq!(w.len(), 3, "{w:?}");
        assert!(w[0].contains("ui.") && w.iter().any(|x| x.contains("ui.sparkles")));
    }

    #[test]
    fn defaults_and_values() {
        let (n, w) = parse("", "c");
        assert_eq!(n, Notify::default());
        assert!(w.is_empty());
        let (n, w) = parse(
            "[ui.notify]\non = [\"approval\"]\nwhen = \"always\"\nmethod = \"command\"\ncommand = [\"notify-send\"]\nstall_after = \"2m\"\n[ui.theme]\nx = 1\n",
            "c",
        );
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(n.on, [Signal::Approval]);
        assert!(!n.unfocused_only);
        assert_eq!(n.method, Method::Command(vec!["notify-send".to_owned()]));
        assert_eq!(n.stall_after, Duration::from_secs(120));
    }

    #[test]
    fn what_is_not_understood_warns_and_falls_back() {
        let (n, w) = parse(
            "[ui.notify]\non = [\"approval\", \"lunch\"]\nmethod = \"command\"\nstall_after = \"5s\"\ncolour = 1\n",
            "/x/config.toml",
        );
        assert_eq!(n.on, [Signal::Approval]);
        assert_eq!(n.method, Method::Off);
        assert_eq!(n.stall_after, Duration::from_secs(900));
        assert_eq!(w.len(), 4, "{w:?}");
        assert!(
            w.iter()
                .all(|l| l.starts_with("warning: /x/config.toml: ui.notify."))
        );
        let (_, w) = parse("not toml [", "f");
        assert_eq!(w.len(), 1);
    }
}
