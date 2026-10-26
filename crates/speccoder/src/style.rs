//! How speccoder's output looks: one place that decides colour,
//! theme, glyphs, width and screen-reader mode, and that every renderer —
//! the reports, the progress lines, the code blocks, the prompts and the
//! dashboard — asks.
//!
//! Styling never changes content: [`strip`] of a painted text is the text.
//! Nothing is told by colour alone: every state keeps its word or glyph.
//! The look is the person's (flags, environment, `[ui]` in their own
//! settings), never the project's.

use std::cell::RefCell;
use std::sync::OnceLock;

use unicode_width::UnicodeWidthChar;

/// A semantic style name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Token {
    /// `verified`.
    Verified,
    /// `regression only`.
    Regression,
    /// `attested by a person`.
    Attested,
    /// Ticked without speccoder evidence.
    NoEvidence,
    /// Skipped.
    Skipped,
    /// Ready to run.
    Ready,
    /// Waits for other tasks.
    Blocked,
    /// Running now.
    Running,
    /// Needs a person.
    Attention,
    /// Failed, refused, an error.
    Danger,
    /// Secondary detail, claims.
    Muted,
    /// Headings, commands to run.
    Accent,
    /// Added lines.
    DiffAdd,
    /// Removed lines.
    DiffDel,
    /// Box borders.
    Border,
    /// The focused box's border.
    BorderFocus,
}

impl Token {
    /// Every token.
    #[cfg(test)]
    pub const ALL: [Self; 16] = [
        Self::Verified,
        Self::Regression,
        Self::Attested,
        Self::NoEvidence,
        Self::Skipped,
        Self::Ready,
        Self::Blocked,
        Self::Running,
        Self::Attention,
        Self::Danger,
        Self::Muted,
        Self::Accent,
        Self::DiffAdd,
        Self::DiffDel,
        Self::Border,
        Self::BorderFocus,
    ];

    /// The seven task states, which must look different in every theme.
    #[cfg(test)]
    pub const STATES: [Self; 7] = [
        Self::Verified,
        Self::Regression,
        Self::Attested,
        Self::NoEvidence,
        Self::Skipped,
        Self::Ready,
        Self::Blocked,
    ];
}

/// A theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    /// The terminal's 16 colours (they follow its palette).
    #[default]
    Auto,
    /// 24-bit, for dark backgrounds.
    Dark,
    /// 24-bit, for light backgrounds.
    Light,
    /// Bold, bright, underlined where it helps.
    HighContrast,
    /// Blue/orange instead of green/red (Okabe–Ito).
    Daltonized,
    /// No colour: bold, dim, underline and reverse only.
    Mono,
}

impl Theme {
    /// Every theme.
    #[cfg(test)]
    pub const ALL: [Self; 6] = [
        Self::Auto,
        Self::Dark,
        Self::Light,
        Self::HighContrast,
        Self::Daltonized,
        Self::Mono,
    ];

    /// The theme named `s` in settings.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "auto" => Self::Auto,
            "dark" => Self::Dark,
            "light" => Self::Light,
            "high-contrast" => Self::HighContrast,
            "daltonized" => Self::Daltonized,
            "mono" => Self::Mono,
            _ => return None,
        })
    }
}

/// A colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    /// One of the terminal's 16 (0–15).
    Ansi(u8),
    /// 24-bit.
    Rgb(u8, u8, u8),
}

/// How a token looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Style {
    /// Foreground.
    pub fg: Option<Color>,
    /// Bold.
    pub bold: bool,
    /// Dim.
    pub dim: bool,
    /// Underlined.
    pub underline: bool,
    /// Reversed.
    pub reverse: bool,
}

const fn fg(c: Color) -> Style {
    Style {
        fg: Some(c),
        bold: false,
        dim: false,
        underline: false,
        reverse: false,
    }
}

impl Style {
    const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    const fn dim(mut self) -> Self {
        self.dim = true;
        self
    }
    const fn underline(mut self) -> Self {
        self.underline = true;
        self
    }
    const fn reverse(mut self) -> Self {
        self.reverse = true;
        self
    }

    /// The SGR parameters (`1;32`), empty for the plain style.
    #[must_use]
    pub fn sgr(&self) -> String {
        let mut p: Vec<String> = Vec::new();
        if self.bold {
            p.push("1".to_owned());
        }
        if self.dim {
            p.push("2".to_owned());
        }
        if self.underline {
            p.push("4".to_owned());
        }
        if self.reverse {
            p.push("7".to_owned());
        }
        match self.fg {
            Some(Color::Ansi(i)) if i < 8 => p.push((30 + u16::from(i)).to_string()),
            Some(Color::Ansi(i)) => p.push((90 + u16::from(i.min(15)) - 8).to_string()),
            Some(Color::Rgb(r, g, b)) => p.push(format!("38;2;{r};{g};{b}")),
            None => {}
        }
        p.join(";")
    }
}

const PLAIN: Style = Style {
    fg: None,
    bold: false,
    dim: false,
    underline: false,
    reverse: false,
};

const fn ansi(i: u8) -> Style {
    fg(Color::Ansi(i))
}

const fn rgb(r: u8, g: u8, b: u8) -> Style {
    fg(Color::Rgb(r, g, b))
}

/// `token`'s style in `theme`.
#[must_use]
pub const fn style(theme: Theme, token: Token) -> Style {
    use Token as T;
    match theme {
        Theme::Auto => match token {
            T::Verified | T::DiffAdd => ansi(2),
            T::Regression => ansi(3),
            T::Attested | T::BorderFocus => ansi(6),
            T::NoEvidence => ansi(5),
            T::Skipped => PLAIN.dim(),
            T::Ready => PLAIN,
            T::Blocked | T::Border => ansi(8),
            T::Running => ansi(4).bold(),
            T::Attention => ansi(3).bold(),
            T::Danger => ansi(1).bold(),
            T::DiffDel => ansi(1),
            T::Muted => PLAIN.dim(),
            T::Accent => PLAIN.bold(),
        },
        // GitHub's palettes.
        Theme::Dark => match token {
            T::Verified | T::DiffAdd => rgb(63, 185, 80),
            T::Regression => rgb(210, 153, 34),
            T::Attested | T::BorderFocus => rgb(88, 166, 255),
            T::NoEvidence => rgb(188, 140, 255),
            T::Skipped => PLAIN.dim(),
            T::Ready => PLAIN,
            T::Blocked | T::Border => rgb(139, 148, 158),
            T::Running => rgb(88, 166, 255).bold(),
            T::Attention => rgb(210, 153, 34).bold(),
            T::Danger => rgb(248, 81, 73).bold(),
            T::DiffDel => rgb(248, 81, 73),
            T::Muted => rgb(139, 148, 158),
            T::Accent => PLAIN.bold(),
        },
        Theme::Light => match token {
            T::Verified | T::DiffAdd => rgb(26, 127, 55),
            T::Regression => rgb(154, 103, 0),
            T::Attested | T::BorderFocus => rgb(9, 105, 218),
            T::NoEvidence => rgb(130, 80, 223),
            T::Skipped => PLAIN.dim(),
            T::Ready => PLAIN,
            T::Blocked | T::Border => rgb(87, 96, 106),
            T::Running => rgb(9, 105, 218).bold(),
            T::Attention => rgb(154, 103, 0).bold(),
            T::Danger => rgb(207, 34, 46).bold(),
            T::DiffDel => rgb(207, 34, 46),
            T::Muted => rgb(87, 96, 106),
            T::Accent => PLAIN.bold(),
        },
        Theme::HighContrast => match token {
            T::Verified | T::DiffAdd => ansi(10).bold(),
            T::Regression => ansi(11).bold().underline(),
            T::Attested | T::BorderFocus => ansi(14).bold(),
            T::NoEvidence => ansi(13).bold().underline(),
            T::Skipped => ansi(15).underline(),
            T::Ready => ansi(15).bold(),
            T::Blocked | T::Border => ansi(15),
            T::Running => ansi(12).bold(),
            T::Attention => ansi(11).bold().reverse(),
            T::Danger => ansi(9).bold().reverse(),
            T::DiffDel => ansi(9).bold(),
            T::Muted => ansi(15),
            T::Accent => ansi(15).bold().underline(),
        },
        // Okabe–Ito: told apart by blue and orange, never green and red.
        Theme::Daltonized => match token {
            T::Verified | T::DiffAdd => rgb(0, 114, 178),
            T::Regression => rgb(230, 159, 0),
            T::Attested | T::BorderFocus => rgb(86, 180, 233),
            T::NoEvidence => rgb(204, 121, 167),
            T::Skipped => PLAIN.dim(),
            T::Ready => PLAIN,
            T::Blocked | T::Border => rgb(153, 153, 153),
            T::Running => rgb(0, 114, 178).bold(),
            T::Attention => rgb(230, 159, 0).bold(),
            T::Danger => rgb(213, 94, 0).bold(),
            T::DiffDel => rgb(213, 94, 0),
            T::Muted => rgb(153, 153, 153),
            T::Accent => PLAIN.bold(),
        },
        Theme::Mono => match token {
            T::Verified | T::Accent => PLAIN.bold(),
            T::Regression => PLAIN.underline(),
            T::Attested => PLAIN.bold().underline(),
            T::NoEvidence | T::Danger => PLAIN.reverse(),
            T::Skipped | T::Muted | T::Border => PLAIN.dim(),
            T::Ready | T::DiffAdd | T::DiffDel => PLAIN,
            T::Blocked => PLAIN.dim().underline(),
            T::Running | T::BorderFocus => PLAIN.bold(),
            T::Attention => PLAIN.bold().reverse(),
        },
    }
}

/// A state's mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    /// `✓`.
    Verified,
    /// `~`.
    Regression,
    /// `A`.
    Attested,
    /// `!`.
    NoEvidence,
    /// `?`.
    TickMissing,
    /// `–`.
    Skipped,
    /// `·`.
    Ready,
    /// `⧗`.
    Blocked,
    /// `◐`.
    Running,
    /// `✗`.
    Failed,
    /// `△`.
    Attention,
}

impl Glyph {
    /// Every glyph.
    #[cfg(test)]
    pub const ALL: [Self; 11] = [
        Self::Verified,
        Self::Regression,
        Self::Attested,
        Self::NoEvidence,
        Self::TickMissing,
        Self::Skipped,
        Self::Ready,
        Self::Blocked,
        Self::Running,
        Self::Failed,
        Self::Attention,
    ];

    /// The Unicode mark.
    #[must_use]
    pub const fn unicode(self) -> &'static str {
        match self {
            Self::Verified => "✓",
            Self::Regression => "~",
            Self::Attested => "A",
            Self::NoEvidence => "!",
            Self::TickMissing => "?",
            Self::Skipped => "–",
            Self::Ready => "·",
            Self::Blocked => "⧗",
            Self::Running => "◐",
            Self::Failed => "✗",
            Self::Attention => "△",
        }
    }

    /// The ASCII mark.
    #[must_use]
    pub const fn ascii(self) -> &'static str {
        match self {
            Self::Verified => "[ok]",
            Self::Regression => "[~]",
            Self::Attested => "[A]",
            Self::NoEvidence => "[!]",
            Self::TickMissing => "[?]",
            Self::Skipped => "[-]",
            Self::Ready => "[ ]",
            Self::Blocked => "[..]",
            Self::Running => "[>]",
            Self::Failed => "[x]",
            Self::Attention => "[!!]",
        }
    }

    /// `ok`, `warning` or `failed`, for a doctor check.
    #[must_use]
    pub const fn word_for_doctor(self) -> &'static str {
        match self {
            Self::Verified => "ok",
            Self::Ready => "optional",
            Self::Regression => "warning",
            _ => "failed",
        }
    }

    /// The word a screen reader hears.
    #[cfg(test)]
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Regression => "regression only",
            Self::Attested => "attested",
            Self::NoEvidence => "no evidence",
            Self::TickMissing => "tick missing",
            Self::Skipped => "skipped",
            Self::Ready => "ready",
            Self::Blocked => "blocked",
            Self::Running => "running",
            Self::Failed => "failed",
            Self::Attention => "needs you",
        }
    }

    /// Its style.
    #[must_use]
    pub const fn token(self) -> Token {
        match self {
            Self::Verified => Token::Verified,
            Self::Regression => Token::Regression,
            Self::Attested => Token::Attested,
            Self::NoEvidence | Self::TickMissing => Token::NoEvidence,
            Self::Skipped => Token::Skipped,
            Self::Ready => Token::Ready,
            Self::Blocked => Token::Blocked,
            Self::Running => Token::Running,
            Self::Failed => Token::Danger,
            Self::Attention => Token::Attention,
        }
    }

    /// The glyph of a task state word (`verified`, `regression_only`, …).
    #[must_use]
    pub fn of_state(state: &str) -> Self {
        match state {
            "verified" => Self::Verified,
            "regression_only" => Self::Regression,
            "attested" => Self::Attested,
            "ticked_without_evidence" => Self::NoEvidence,
            "evidence_without_tick" => Self::TickMissing,
            "skipped" => Self::Skipped,
            "ready" => Self::Ready,
            "running" => Self::Running,
            "failed" | "verification_failed" | "error" => Self::Failed,
            "needs_approval" | "suspected_gaming" => Self::Attention,
            _ => Self::Blocked,
        }
    }
}

/// How one stream looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    /// Escape codes are written.
    pub color: bool,
    /// The theme.
    pub theme: Theme,
    /// ASCII glyphs and borders.
    pub ascii: bool,
    /// Columns.
    pub width: usize,
    /// Labelled linear lines, no glyph-only marks.
    pub screen_reader: bool,
}

impl Look {
    /// No colour, Unicode, 100 columns: what output is when nothing was
    /// decided (and what tests compare against).
    pub const PLAIN: Self = Self {
        color: false,
        theme: Theme::Auto,
        ascii: false,
        width: 100,
        screen_reader: false,
    };

    /// `text` in `token`'s style (as is without colour).
    #[must_use]
    pub fn paint(&self, token: Token, text: &str) -> String {
        if !self.color || text.is_empty() {
            return text.to_owned();
        }
        let sgr = style(self.theme, token).sgr();
        if sgr.is_empty() {
            text.to_owned()
        } else {
            format!("\u{1b}[{sgr}m{text}\u{1b}[0m")
        }
    }

    /// The glyph as a column cell: padded so marks line up (ASCII marks
    /// differ in width), painted.
    #[must_use]
    pub fn cell(&self, g: Glyph) -> String {
        let mark = self.mark(g);
        let cols = if self.ascii { 4 } else { 1 };
        let padded = format!("{mark}{}", " ".repeat(cols - mark.len().min(cols)));
        self.paint(g.token(), &padded)
    }

    /// The mark, unpainted.
    #[must_use]
    pub const fn mark(&self, g: Glyph) -> &'static str {
        if self.ascii { g.ascii() } else { g.unicode() }
    }

    /// `…` or `...`.
    #[must_use]
    pub const fn ellipsis(&self) -> &'static str {
        if self.ascii { "..." } else { "…" }
    }

    /// `text` cut to `cols` display columns, ending in the ellipsis.
    #[must_use]
    pub fn fit(&self, text: &str, cols: usize) -> String {
        fit(text, cols, self.ellipsis())
    }
}

/// `text` with speccoder's symbols in ASCII: the glyph table's marks, the
/// separators and arrows its lines use, and box drawing.
#[must_use]
pub fn asciify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '✓' => out.push_str("[ok]"),
            '✗' => out.push_str("[x]"),
            '△' => out.push_str("[!!]"),
            '⧗' => out.push_str("[..]"),
            '◐' => out.push_str("[>]"),
            '⏸' => out.push_str("[||]"),
            '▶' => out.push('>'),
            '»' => out.push_str(">>"),
            '·' | '–' | '—' | '─' | '━' => out.push('-'),
            '…' => out.push_str("..."),
            '→' => out.push_str("->"),
            '■' => out.push('#'),
            '□' => out.push('.'),
            '│' | '┃' => out.push('|'),
            '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼' => out.push('+'),
            '“' | '”' => out.push('"'),
            '∴' => out.push_str(":."),
            '‘' | '’' => out.push('\''),
            c => out.push(c),
        }
    }
    out
}

/// Display width of `text` (escape codes not counted).
#[must_use]
pub fn width(text: &str) -> usize {
    strip(text).chars().map(|c| c.width().unwrap_or(0)).sum()
}

/// `text` cut to `cols` display columns, never splitting a character.
#[must_use]
pub fn fit(text: &str, cols: usize, ellipsis: &str) -> String {
    if width(text) <= cols {
        return text.to_owned();
    }
    let room = cols.saturating_sub(width(ellipsis));
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > room {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push_str(ellipsis);
    out
}

/// `text` without SGR escape codes.
#[must_use]
pub fn strip(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for d in chars.by_ref() {
                if ('@'..='~').contains(&d) {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// What the colour decision reads.
#[derive(Debug, Clone, Default)]
pub struct Env {
    /// `NO_COLOR`.
    pub no_color: Option<String>,
    /// `FORCE_COLOR`.
    pub force_color: Option<String>,
    /// `CLICOLOR_FORCE`.
    pub clicolor_force: Option<String>,
    /// `CLICOLOR`.
    pub clicolor: Option<String>,
    /// `TERM`.
    pub term: Option<String>,
}

impl Env {
    /// This process's environment.
    #[must_use]
    pub fn current() -> Self {
        let var = |k: &str| std::env::var(k).ok();
        Self {
            no_color: var("NO_COLOR"),
            force_color: var("FORCE_COLOR"),
            clicolor_force: var("CLICOLOR_FORCE"),
            clicolor: var("CLICOLOR"),
            term: var("TERM"),
        }
    }
}

/// `--color` and `[ui] color`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Choice {
    /// Decide from the environment and the stream.
    #[default]
    Auto,
    /// Always.
    Always,
    /// Never.
    Never,
}

/// Whether a stream gets colour: the flag, the person's setting, then
/// `NO_COLOR`, `FORCE_COLOR`/`CLICOLOR_FORCE`, `CLICOLOR=0`, `TERM=dumb`,
/// then whether it is a terminal.
#[must_use]
pub fn decide(flag: Choice, setting: Choice, env: &Env, terminal: bool) -> bool {
    let set = |v: &Option<String>| v.as_deref().is_some_and(|v| !v.is_empty());
    let forced = |v: &Option<String>| set(v) && v.as_deref() != Some("0");
    for choice in [flag, setting] {
        match choice {
            Choice::Always => return true,
            Choice::Never => return false,
            Choice::Auto => {}
        }
    }
    if set(&env.no_color) {
        return false;
    }
    if forced(&env.force_color) || forced(&env.clicolor_force) {
        return true;
    }
    if env.clicolor.as_deref() == Some("0") || env.term.as_deref() == Some("dumb") {
        return false;
    }
    terminal
}

/// Whether the locale asks for ASCII: one is set and it is not UTF-8.
#[must_use]
pub fn locale_is_ascii(lc_all: Option<&str>, lc_ctype: Option<&str>, lang: Option<&str>) -> bool {
    let locale = [lc_all, lc_ctype, lang]
        .into_iter()
        .flatten()
        .find(|v| !v.is_empty());
    locale.is_some_and(|l| {
        let l = l.to_ascii_lowercase();
        !(l.contains("utf-8") || l.contains("utf8"))
    })
}

/// The streams' looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Looks {
    /// stdout.
    pub out: Look,
    /// stderr.
    pub err: Look,
}

static INSTALLED: OnceLock<Looks> = OnceLock::new();

thread_local! {
    static OVERRIDE: RefCell<Option<Look>> = const { RefCell::new(None) };
}

/// Sets the process's looks (once, from `main`).
pub fn install(looks: Looks) {
    let _ = INSTALLED.set(looks);
}

/// stdout's look.
#[must_use]
pub fn out() -> Look {
    OVERRIDE
        .with(|o| *o.borrow())
        .or_else(|| INSTALLED.get().map(|l| l.out))
        .unwrap_or(Look::PLAIN)
}

/// stderr's look.
#[must_use]
pub fn err() -> Look {
    OVERRIDE
        .with(|o| *o.borrow())
        .or_else(|| INSTALLED.get().map(|l| l.err))
        .unwrap_or(Look::PLAIN)
}

/// Runs `f` with `look` for both streams on this thread (tests).
#[cfg(test)]
pub fn with<T>(look: Look, f: impl FnOnce() -> T) -> T {
    let before = OVERRIDE.with(|o| o.replace(Some(look)));
    let out = f();
    OVERRIDE.with(|o| *o.borrow_mut() = before);
    out
}

/// A token's style for the dashboard.
#[must_use]
pub fn tui_style(look: &Look, token: Token) -> ratatui::style::Style {
    use ratatui::style::{Color as C, Modifier, Style as S};
    let st = style(look.theme, token);
    let mut out = S::default();
    if look.color {
        out = match st.fg {
            Some(Color::Ansi(i)) => out.fg(C::Indexed(i)),
            Some(Color::Rgb(r, g, b)) => out.fg(C::Rgb(r, g, b)),
            None => out,
        };
    }
    for (on, m) in [
        (st.bold, Modifier::BOLD),
        (st.dim, Modifier::DIM),
        (st.underline, Modifier::UNDERLINED),
        (st.reverse, Modifier::REVERSED),
    ] {
        if on {
            out = out.add_modifier(m);
        }
    }
    out
}

/// The terminal's width when `terminal`, else `COLUMNS`, else 100.
#[must_use]
pub fn columns(terminal: bool, env_columns: Option<&str>) -> usize {
    let size = terminal
        .then(ratatui::crossterm::terminal::size)
        .and_then(Result::ok)
        .map(|(w, _)| usize::from(w))
        .filter(|w| *w > 0);
    size.or_else(|| {
        env_columns
            .and_then(|c| c.trim().parse().ok())
            .filter(|w: &usize| *w > 0)
    })
    .unwrap_or(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seven_states_look_different_in_every_theme() {
        for theme in Theme::ALL {
            let styles: std::collections::HashSet<Style> =
                Token::STATES.iter().map(|t| style(theme, *t)).collect();
            assert_eq!(styles.len(), 7, "{theme:?}");
        }
        for t in Token::ALL {
            assert!(
                style(Theme::Mono, t).fg.is_none(),
                "mono has no colour: {t:?}"
            );
        }
    }

    #[test]
    fn painting_never_changes_the_text() {
        for theme in Theme::ALL {
            let look = Look {
                color: true,
                theme,
                ..Look::PLAIN
            };
            for t in Token::ALL {
                assert_eq!(strip(&look.paint(t, "T001 ✓ x")), "T001 ✓ x");
            }
        }
        assert_eq!(Look::PLAIN.paint(Token::Danger, "x"), "x");
        assert!(
            Look {
                color: true,
                ..Look::PLAIN
            }
            .paint(Token::Danger, "x")
            .starts_with("\u{1b}[1;31m")
        );
    }

    #[test]
    fn colour_follows_flag_setting_and_environment_in_that_order() {
        let env = |pairs: &[(&str, &str)]| {
            let get = |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| (*v).to_owned())
            };
            Env {
                no_color: get("NO_COLOR"),
                force_color: get("FORCE_COLOR"),
                clicolor_force: get("CLICOLOR_FORCE"),
                clicolor: get("CLICOLOR"),
                term: get("TERM"),
            }
        };
        use Choice::{Always, Auto, Never};
        assert!(decide(Auto, Auto, &env(&[]), true));
        assert!(!decide(Auto, Auto, &env(&[]), false));
        assert!(!decide(Auto, Auto, &env(&[("NO_COLOR", "1")]), true));
        assert!(
            decide(Auto, Auto, &env(&[("NO_COLOR", "")]), true),
            "empty is unset"
        );
        assert!(
            decide(Always, Auto, &env(&[("NO_COLOR", "1")]), false),
            "the flag wins"
        );
        assert!(!decide(Auto, Never, &env(&[]), true), "then the setting");
        assert!(decide(Auto, Auto, &env(&[("FORCE_COLOR", "1")]), false));
        assert!(!decide(Auto, Auto, &env(&[("FORCE_COLOR", "0")]), false));
        assert!(!decide(
            Auto,
            Auto,
            &env(&[("FORCE_COLOR", "1"), ("NO_COLOR", "1")]),
            true
        ));
        assert!(decide(Auto, Auto, &env(&[("CLICOLOR_FORCE", "1")]), false));
        assert!(!decide(Auto, Auto, &env(&[("CLICOLOR", "0")]), true));
        assert!(!decide(Auto, Auto, &env(&[("TERM", "dumb")]), true));
        assert!(
            !decide(Never, Always, &env(&[]), true),
            "the flag beats the setting"
        );
    }

    #[test]
    fn width_is_display_width_and_cuts_never_split_a_character() {
        assert_eq!(width("日本"), 4);
        assert_eq!(fit("日本語のタイトル", 7, "…"), "日本語…");
        assert_eq!(fit("short", 10, "…"), "short");
        assert_eq!(fit("abcdefghij", 5, "..."), "ab...");
        let painted = Look {
            color: true,
            ..Look::PLAIN
        }
        .paint(Token::Verified, "ok");
        assert_eq!(width(&painted), 2);
    }

    #[test]
    fn glyphs_have_ascii_forms_and_words() {
        for g in Glyph::ALL {
            assert!(g.ascii().is_ascii() && g.ascii().starts_with('['), "{g:?}");
            assert!(!g.word().is_empty());
        }
        let ascii = Look {
            ascii: true,
            ..Look::PLAIN
        };
        assert_eq!(ascii.mark(Glyph::of_state("verified")), "[ok]");
        assert_eq!(Look::PLAIN.mark(Glyph::of_state("blocked")), "⧗");
    }

    #[test]
    fn a_set_non_utf8_locale_means_ascii() {
        assert!(locale_is_ascii(None, None, Some("C")));
        assert!(!locale_is_ascii(None, None, Some("en_US.UTF-8")));
        assert!(!locale_is_ascii(Some("C.utf8"), None, Some("C")));
        assert!(!locale_is_ascii(None, None, None), "unset: Unicode");
        assert_eq!(columns(false, Some("60")), 60);
        assert_eq!(columns(false, None), 100);
    }
}
