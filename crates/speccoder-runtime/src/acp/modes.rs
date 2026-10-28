//! An attempt's permission mode: what the agent advertises, what speccoder
//! may ask it for, and what the run says about it.
//!
//! speccoder asks for exactly one thing, and only when the person configured
//! no mode (`[executor] mode`): the agent's own ask-first value, which sends
//! *more* calls to speccoder's policy. A mode the person configured — in the
//! agent's own settings, or as `mode` — is kept. Which value asks first, and
//! where an adapter starts when nothing is configured, is recorded per
//! adapter version from that adapter's own output ([`PROFILES`]); nothing
//! here maps one vendor's mode onto another's. An agent left in a mode that
//! decides without asking is said so on the run: its empty Needs You then
//! proves nothing.
//!
//! Adapted from devplane's `acp::modes` (same owner, MIT OR Apache-2.0).

use serde::{Deserialize, Serialize};

/// How an agent advertises its mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Via {
    /// A session config option of this id (`session/set_config_option`).
    Config(String),
    /// The older session modes (`session/set_mode`).
    Modes,
}

/// Whether a mode asks before the agent acts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reading {
    /// Calls reach speccoder as permission requests.
    Asks,
    /// The agent decides some calls itself.
    DecidesWithoutYou,
    /// No recording says.
    NotKnown,
}

/// What one adapter version advertises, recorded from its own output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeProfile {
    /// The preset it belongs to.
    pub agent: &'static str,
    /// The `initialize` response's `agentInfo` name and version, exactly.
    pub agent_info: (&'static str, &'static str),
    /// `Some(id)` for a config option, `None` for session modes.
    pub config_id: Option<&'static str>,
    /// Where the adapter opens when nothing is configured.
    pub unconfigured_start: &'static str,
    /// The value that asks before acting.
    pub ask_first: Option<&'static str>,
    /// Each advertised value: `None` asks, `Some(what)` decides that alone.
    pub values: &'static [(&'static str, Option<&'static str>)],
    /// A second option that can turn asking off whatever the mode (its id,
    /// and the value that keeps asking on).
    pub guard: Option<(&'static str, &'static str)>,
    /// What makes the agent ask at all, set where it starts (a variable
    /// and its value) when the person configured no mode: an agent whose
    /// modes do not govern permissions.
    pub launch_env: Option<(&'static str, &'static str)>,
}

impl ModeProfile {
    /// The channel it advertises its mode on.
    #[must_use]
    pub fn via(&self) -> Via {
        match self.config_id {
            Some(id) => Via::Config(id.to_owned()),
            None => Via::Modes,
        }
    }

    /// How this value reads, and what it decides; *not known* for a value
    /// the recording does not contain.
    #[must_use]
    pub fn reading(&self, value: &str) -> (Reading, Option<String>) {
        match self.values.iter().find(|(v, _)| *v == value) {
            Some((_, None)) => (Reading::Asks, None),
            Some((_, Some(what))) => (Reading::DecidesWithoutYou, Some((*what).to_owned())),
            None => (Reading::NotKnown, None),
        }
    }

    fn offers_exactly(&self, ad: &Advertised) -> bool {
        let mut recorded: Vec<&str> = self.values.iter().map(|(v, _)| *v).collect();
        let mut live: Vec<&str> = ad.values.iter().map(|(v, _)| v.as_str()).collect();
        recorded.sort_unstable();
        live.sort_unstable();
        recorded == live
    }
}

/// OpenCode's permission rules, every one set to ask (its modes say what it
/// works on, not whether it asks).
const OPENCODE_ASKS: &str = r#"{"permission":{"read":"ask","edit":"ask","bash":"ask","webfetch":"ask","websearch":"ask","external_directory":"ask"}}"#;

/// One profile per recorded adapter version, each value taken from that
/// version's own advertisement (`tests/fixtures/acp/`).
pub const PROFILES: &[ModeProfile] = &[
    // Recorded here 2026-10-28: `agent` asked for an edit and a command;
    // `autopilot` "enables allow-all"; `allow_all` starts `off`.
    ModeProfile {
        agent: "copilot",
        agent_info: ("Copilot", "1.0.105"),
        config_id: Some("mode"),
        unconfigured_start: "https://agentclientprotocol.com/protocol/session-modes#agent",
        ask_first: Some("https://agentclientprotocol.com/protocol/session-modes#agent"),
        values: &[
            (
                "https://agentclientprotocol.com/protocol/session-modes#agent",
                None,
            ),
            (
                "https://agentclientprotocol.com/protocol/session-modes#plan",
                None,
            ),
            (
                "https://agentclientprotocol.com/protocol/session-modes#autopilot",
                Some("everything (allow-all)"),
            ),
        ],
        guard: Some(("allow_all", "off")),
        launch_env: None,
    },
    // Recorded here 2026-10-28 (2.1.4, again on 2.2.0): in `build` it ran a
    // command and wrote a file without asking; with every permission rule
    // set to `ask` it asked first.
    ModeProfile {
        agent: "opencode",
        agent_info: ("OpenCode", "2.2.0"),
        config_id: Some("mode"),
        unconfigured_start: "build",
        ask_first: None,
        values: &[("build", None), ("plan", None)],
        guard: None,
        launch_env: Some(("OPENCODE_CONFIG_CONTENT", OPENCODE_ASKS)),
    },
    // claude-agent-acp passes `default` to Claude Code when no
    // `permissions.defaultMode` is set, so any other start was configured.
    ModeProfile {
        agent: "claude",
        agent_info: ("@agentclientprotocol/claude-agent-acp", "0.85.1"),
        config_id: Some("mode"),
        unconfigured_start: "default",
        ask_first: Some("default"),
        values: &[
            ("default", None),
            ("acceptEdits", Some("edits")),
            ("plan", None),
            ("auto", Some("a classifier decides")),
            ("bypassPermissions", Some("everything")),
        ],
        guard: None,
        launch_env: None,
    },
    // codex-acp starts every session in `agent` (Auto review, where a
    // reviewer model answers approvals) unless `INITIAL_AGENT_MODE` is set.
    ModeProfile {
        agent: "codex",
        agent_info: ("@agentclientprotocol/codex-acp", "2.1.1"),
        config_id: Some("mode"),
        unconfigured_start: "agent",
        ask_first: Some("read-only"),
        values: &[
            ("read-only", None),
            ("workspace-write", Some("workspace edits")),
            ("agent", Some("a reviewer model decides")),
            ("agent-full-access", Some("everything")),
        ],
        guard: None,
        launch_env: None,
    },
    // Gemini CLI advertises session modes only.
    ModeProfile {
        agent: "gemini",
        agent_info: ("gemini-cli", "0.62.0"),
        config_id: None,
        unconfigured_start: "default",
        ask_first: Some("default"),
        values: &[
            ("default", None),
            ("autoEdit", Some("edits")),
            ("yolo", Some("everything")),
        ],
        guard: None,
        launch_env: None,
    },
];

/// The profiles of the agent that named itself `name`: exactly this
/// version first, then any recorded version of it (which applies only when
/// the live advertisement offers exactly its values — [`profile_for`]).
fn candidates<'a>(
    name: &'a str,
    version: &'a str,
) -> impl Iterator<Item = &'static ModeProfile> + 'a {
    let exact = PROFILES
        .iter()
        .filter(move |p| p.agent_info == (name, version));
    let others = PROFILES
        .iter()
        .filter(move |p| p.agent_info.0 == name && p.agent_info.1 != version);
    exact.chain(others)
}

/// The profile for the agent that named itself so: its recording at exactly
/// this version, or a recording of another version whose values the live
/// set-up answer offers exactly — the same vocabulary, so the same reading.
/// `None` is unprofiled: nothing is asked for.
#[must_use]
pub fn profile_for(
    name: &str,
    version: &str,
    config_options: &serde_json::Value,
    modes: &serde_json::Value,
) -> Option<&'static ModeProfile> {
    candidates(name, version).find(|p| {
        p.agent_info.1 == version
            || Advertised::read(config_options, modes, Some(p))
                .is_some_and(|a| p.offers_exactly(&a))
    })
}

/// The profile to start an agent with (its launch variables), by preset.
#[must_use]
pub fn launch_profile(agent: &str) -> Option<&'static ModeProfile> {
    PROFILES.iter().find(|p| p.agent == agent)
}

/// The mode a set-up response advertised: through which channel, its
/// current value, and every value with the agent's own name for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advertised {
    /// The channel.
    pub via: Via,
    /// Its value now.
    pub current: String,
    /// Each value and the agent's name for it.
    pub values: Vec<(String, String)>,
}

impl Advertised {
    /// Reads a set-up response's `configOptions` and `modes` (as JSON). A
    /// profiled agent is read only through its profile's channel — a missing
    /// config option never falls back to modes. An unprofiled agent is read
    /// from its single select option of category `mode`, else its modes.
    #[must_use]
    pub fn read(
        config_options: &serde_json::Value,
        modes: &serde_json::Value,
        profile: Option<&ModeProfile>,
    ) -> Option<Self> {
        match profile.map(ModeProfile::via) {
            Some(Via::Config(id)) => from_config(config_options, |o| o["id"] == id.as_str()),
            Some(Via::Modes) => from_modes(modes),
            None => {
                let options = config_options.as_array().map_or(&[][..], Vec::as_slice);
                let selects: Vec<&serde_json::Value> = options
                    .iter()
                    .filter(|o| o["category"] == "mode" && o["type"] == "select")
                    .collect();
                match selects.as_slice() {
                    [one] => from_config(config_options, |o| o["id"] == one["id"]),
                    [] if !options.iter().any(|o| o["category"] == "mode") => from_modes(modes),
                    // Several, or only a boolean one: nothing is read.
                    _ => None,
                }
            }
        }
    }

    /// The agent's name for a value, or the value itself.
    #[must_use]
    pub fn name_of(&self, value: &str) -> String {
        self.values
            .iter()
            .find(|(v, _)| v == value)
            .map_or_else(|| value.to_owned(), |(_, n)| n.clone())
    }

    /// Whether it offers `value`.
    #[must_use]
    pub fn offers(&self, value: &str) -> bool {
        self.values.iter().any(|(v, _)| v == value)
    }

    /// The current value of this mode option in a full option list (a
    /// `session/set_config_option` answer).
    #[must_use]
    pub fn current_in(&self, config_options: &serde_json::Value) -> Option<String> {
        let Via::Config(id) = &self.via else {
            return None;
        };
        current_of(config_options, id)
    }
}

/// The current value of option `id` in a list of config options.
#[must_use]
pub fn current_of(config_options: &serde_json::Value, id: &str) -> Option<String> {
    config_options.as_array()?.iter().find(|o| o["id"] == id)?["currentValue"]
        .as_str()
        .map(str::to_owned)
}

fn from_config(
    options: &serde_json::Value,
    pick: impl Fn(&serde_json::Value) -> bool,
) -> Option<Advertised> {
    let option = options.as_array()?.iter().find(|o| pick(o))?;
    // A boolean option is never requested.
    if option["type"] != "select" {
        return None;
    }
    let mut values = Vec::new();
    for o in option["options"].as_array()? {
        // Options may be grouped.
        match o["options"].as_array() {
            Some(group) => values.extend(group.iter().filter_map(value_of)),
            None => values.extend(value_of(o)),
        }
    }
    Some(Advertised {
        via: Via::Config(option["id"].as_str()?.to_owned()),
        current: option["currentValue"].as_str()?.to_owned(),
        values,
    })
}

fn value_of(o: &serde_json::Value) -> Option<(String, String)> {
    let v = o["value"].as_str()?;
    Some((v.to_owned(), o["name"].as_str().unwrap_or(v).to_owned()))
}

fn from_modes(modes: &serde_json::Value) -> Option<Advertised> {
    let current = modes["currentModeId"].as_str()?;
    Some(Advertised {
        via: Via::Modes,
        current: current.to_owned(),
        values: modes["availableModes"]
            .as_array()?
            .iter()
            .filter_map(|m| {
                let id = m["id"].as_str()?;
                Some((id.to_owned(), m["name"].as_str().unwrap_or(id).to_owned()))
            })
            .collect(),
    })
}

/// Why an attempt is in the mode its statement names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    /// The person set none; speccoder asked for the ask-first mode.
    AskedForAskFirst,
    /// The session opened in its ask-first mode; nothing was asked.
    AsksAlready,
    /// `[executor] mode` named this mode.
    YouChose,
    /// `[executor] mode` named a mode the agent does not offer.
    ChoseUnoffered,
    /// The session opened in a mode other than the adapter's unconfigured
    /// start: the person configured it in the agent's own settings.
    YouSetIt,
    /// speccoder asked for the ask-first mode and the agent refused.
    Refused,
    /// speccoder asked for the ask-first mode and the agent did not answer.
    NoAnswer,
    /// The agent advertises no mode known to ask first.
    NoAskFirst,
    /// The agent advertises no mode at all.
    NoneStated,
    /// No recorded profile for this agent and version.
    Unprofiled,
}

/// What speccoder does at set-up, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModePlan {
    /// The one value to request, if any.
    pub request: Option<String>,
    /// Why.
    pub why: Why,
}

impl ModePlan {
    /// The decision table; the first row that matches wins. Its only
    /// sources for a requested value are the profile's ask-first value and
    /// the person's own preference.
    #[must_use]
    pub fn of(
        profile: Option<&ModeProfile>,
        advertised: Option<&Advertised>,
        preference: Option<&str>,
    ) -> Self {
        let none = |why| Self { request: None, why };
        let Some(ad) = advertised else {
            return none(Why::NoneStated);
        };
        if let Some(p) = preference {
            return match (ad.offers(p), p == ad.current) {
                (false, _) => none(Why::ChoseUnoffered),
                (true, true) => none(Why::YouChose),
                (true, false) => Self {
                    request: Some(p.to_owned()),
                    why: Why::YouChose,
                },
            };
        }
        let Some(profile) = profile else {
            return none(Why::Unprofiled);
        };
        if profile.ask_first == Some(ad.current.as_str())
            || (profile.ask_first.is_none() && profile.launch_env.is_some())
        {
            return none(Why::AsksAlready);
        }
        if ad.current != profile.unconfigured_start {
            return none(Why::YouSetIt);
        }
        match profile.ask_first.filter(|a| ad.offers(a)) {
            None => none(Why::NoAskFirst),
            Some(a) => Self {
                request: Some(a.to_owned()),
                why: Why::AskedForAskFirst,
            },
        }
    }
}

/// How the one request ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The agent's value afterwards, which may differ from the one asked for.
    Confirmed(String),
    /// The agent's refusal.
    Refused(String),
    /// No answer in time.
    NoAnswer,
}

/// What an attempt says about its permission mode: the mode in the agent's
/// own words, whether it asks, and why it is the mode it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statement {
    /// The agent's name.
    pub agent: String,
    /// The agent's own id for the mode.
    pub mode: Option<String>,
    /// The agent's own name for it.
    pub name: Option<String>,
    /// Whether it asks.
    pub reading: Reading,
    /// What it decides without asking, when it does.
    pub decides: Option<String>,
    /// Why it is in it.
    pub why: Why,
    /// The refusal, or the preference the agent does not offer.
    pub detail: Option<String>,
}

fn one_line(s: &str) -> String {
    let line = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > 80 {
        format!("{}…", line.chars().take(79).collect::<String>())
    } else {
        line
    }
}

impl Statement {
    /// The sentence the run shows. Agent text is clipped to one line.
    #[must_use]
    pub fn sentence(&self) -> String {
        let agent = one_line(&self.agent);
        let mode = one_line(
            self.name
                .as_deref()
                .or(self.mode.as_deref())
                .unwrap_or("an unnamed mode"),
        );
        let detail = one_line(self.detail.as_deref().unwrap_or(""));
        let what = one_line(self.decides.as_deref().unwrap_or("what it decides"));
        let deciding = self.reading == Reading::DecidesWithoutYou;
        let tail = if deciding {
            format!(" — decides without asking: {what}")
        } else {
            String::new()
        };
        match self.why {
            Why::AskedForAskFirst if self.reading == Reading::Asks => {
                format!("asked {agent} to ask before acting — you set no mode")
            }
            Why::AskedForAskFirst => {
                format!("asked {agent} to ask before acting — it is in {mode}{tail}")
            }
            Why::AsksAlready if deciding => format!("{agent} is in {mode}{tail}"),
            Why::AsksAlready => format!("{agent} asks before acting: {mode}"),
            Why::YouChose if self.reading == Reading::NotKnown => {
                format!("you chose {mode} — not known whether {agent} asks")
            }
            Why::YouChose => format!("you chose {mode}{tail}"),
            Why::ChoseUnoffered => {
                format!("you chose {detail}; {agent} does not offer it — it is in {mode}{tail}")
            }
            Why::YouSetIt if deciding => {
                format!("{agent} decides without asking: {mode} ({what}) — set in its own settings")
            }
            Why::YouSetIt if self.reading == Reading::NotKnown => {
                format!("{agent} is in {mode} — set in its own settings; not known whether it asks")
            }
            Why::YouSetIt => format!("{agent} is in {mode} — set in its own settings"),
            Why::Refused => format!(
                "asked {agent} to ask before acting; it refused: {detail} — it is in {mode}{tail}"
            ),
            Why::NoAnswer => format!(
                "asked {agent} to ask before acting; it did not answer — it is in {mode}{tail}"
            ),
            Why::NoAskFirst => {
                format!("{agent} advertises no mode known to ask first; it is in {mode}{tail}")
            }
            Why::NoneStated => {
                format!("{agent} states no permission mode — not known whether it asks")
            }
            Why::Unprofiled => format!("{agent} is in {mode} — not known whether it asks"),
        }
    }

    /// Whether calls reach speccoder: `Some(false)` when the agent decides
    /// some itself, `None` when not known.
    #[must_use]
    pub const fn asks(&self) -> Option<bool> {
        match self.reading {
            Reading::Asks => Some(true),
            Reading::DecidesWithoutYou => Some(false),
            Reading::NotKnown => None,
        }
    }
}

/// What the set-up came to, in a sentence's data. `config_options` is the
/// latest full list the agent sent (for the profile's guard).
#[must_use]
pub fn statement(
    agent: &str,
    profile: Option<&ModeProfile>,
    advertised: Option<&Advertised>,
    plan: &ModePlan,
    outcome: Option<&Outcome>,
    preference: Option<&str>,
    config_options: &serde_json::Value,
) -> Statement {
    let current = match (outcome, advertised) {
        (Some(Outcome::Confirmed(v)), _) => Some(v.clone()),
        (_, Some(ad)) => Some(ad.current.clone()),
        _ => None,
    };
    let (mut reading, mut decides) = match (&current, profile) {
        // A launch rule makes the agent ask only when speccoder set it.
        (Some(_), Some(p)) if p.launch_env.is_some() && preference.is_some() => {
            (Reading::NotKnown, None)
        }
        (Some(v), Some(p)) => p.reading(v),
        _ => (Reading::NotKnown, None),
    };
    // A guard turned off decides whatever the mode says.
    if let Some((id, asks)) = profile.and_then(|p| p.guard)
        && let Some(now) = current_of(config_options, id)
        && now != asks
    {
        reading = Reading::DecidesWithoutYou;
        decides = Some(format!("everything ({id} is {now})"));
    }
    let (why, detail) = match outcome {
        Some(Outcome::Refused(reason)) => (Why::Refused, Some(reason.clone())),
        Some(Outcome::NoAnswer) => (Why::NoAnswer, None),
        _ if plan.why == Why::ChoseUnoffered => {
            (Why::ChoseUnoffered, preference.map(str::to_owned))
        }
        _ => (plan.why, None),
    };
    Statement {
        agent: agent.to_owned(),
        name: match (&current, advertised) {
            (Some(v), Some(ad)) => Some(ad.name_of(v)),
            _ => None,
        },
        mode: current,
        reading,
        decides,
        why,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn profile(agent: &str) -> &'static ModeProfile {
        PROFILES
            .iter()
            .find(|p| p.agent == agent)
            .unwrap_or_else(|| panic!("{agent}"))
    }

    fn ad(current: &str, values: &[&str]) -> Advertised {
        Advertised {
            via: Via::Config("mode".into()),
            current: current.into(),
            values: values
                .iter()
                .map(|v| ((*v).to_owned(), (*v).to_owned()))
                .collect(),
        }
    }

    const CODEX: &[&str] = &["read-only", "workspace-write", "agent", "agent-full-access"];

    #[test]
    fn the_plan_table_row_by_row() {
        let p = Some(profile("codex"));
        let start = ad("agent", CODEX);
        let row = |prof, a: Option<&Advertised>, pref| ModePlan::of(prof, a, pref);
        assert_eq!(row(p, None, None).why, Why::NoneStated);
        let r = row(p, Some(&start), Some("nope"));
        assert_eq!((r.request, r.why), (None, Why::ChoseUnoffered));
        let r = row(p, Some(&start), Some("agent"));
        assert_eq!((r.request, r.why), (None, Why::YouChose));
        let r = row(p, Some(&start), Some("workspace-write"));
        assert_eq!(
            (r.request.as_deref(), r.why),
            (Some("workspace-write"), Why::YouChose)
        );
        assert_eq!(row(None, Some(&start), None).why, Why::Unprofiled);
        assert_eq!(
            row(p, Some(&ad("read-only", CODEX)), None).why,
            Why::AsksAlready
        );
        let r = row(p, Some(&ad("agent-full-access", CODEX)), None);
        assert_eq!((r.request, r.why), (None, Why::YouSetIt));
        let r = row(p, Some(&ad("agent", &["agent", "workspace-write"])), None);
        assert_eq!((r.request, r.why), (None, Why::NoAskFirst));
        let r = row(p, Some(&start), None);
        assert_eq!(
            (r.request.as_deref(), r.why),
            (Some("read-only"), Why::AskedForAskFirst)
        );
    }

    #[test]
    fn never_requests_a_deciding_mode_unasked() {
        for profile in PROFILES {
            let values: Vec<&str> = profile.values.iter().map(|(v, _)| *v).collect();
            for current in &values {
                let a = Advertised {
                    via: profile.via(),
                    current: (*current).to_owned(),
                    values: values
                        .iter()
                        .map(|v| ((*v).to_owned(), (*v).to_owned()))
                        .collect(),
                };
                let plan = ModePlan::of(Some(profile), Some(&a), None);
                assert!(
                    plan.request.is_none() || plan.request.as_deref() == profile.ask_first,
                    "{} at {current} would request {:?}",
                    profile.agent,
                    plan.request
                );
                if let Some(r) = &plan.request {
                    assert_eq!(profile.reading(r).0, Reading::Asks);
                }
            }
        }
        assert_eq!(
            ModePlan::of(None, Some(&ad("x", &["x", "y"])), None).request,
            None
        );
    }

    #[test]
    fn another_version_is_profiled_only_by_the_same_vocabulary() {
        let options = |values: &[&str], current: &str| {
            json!([{ "id": "mode", "category": "mode", "type": "select", "currentValue": current,
                     "options": values.iter().map(|v| json!({ "value": v, "name": v })).collect::<Vec<_>>() }])
        };
        let name = "@agentclientprotocol/codex-acp";
        // The recorded version: its profile, whatever it offers.
        assert!(profile_for(name, "2.1.1", &json!([]), &json!(null)).is_some());
        // A newer one offering exactly the recorded values reads the same.
        let same = options(CODEX, "agent");
        assert_eq!(
            profile_for(name, "2.3.0", &same, &json!(null)).map(|p| p.agent),
            Some("codex")
        );
        // One that offers another vocabulary is unprofiled: nothing is asked.
        let other = options(&["read-only", "agent", "yolo"], "agent");
        assert_eq!(profile_for(name, "2.3.0", &other, &json!(null)), None);
        assert_eq!(profile_for("Kimi", "1.0.0", &same, &json!(null)), None);
    }

    #[test]
    fn statements_say_when_the_agent_decides_alone() {
        let copilot = profile("copilot");
        let agent = "https://agentclientprotocol.com/protocol/session-modes#agent";
        let a = Advertised {
            via: Via::Config("mode".into()),
            current: agent.into(),
            values: vec![(agent.into(), "Agent".into())],
        };
        let plan = ModePlan::of(Some(copilot), Some(&a), None);
        assert_eq!(plan.why, Why::AsksAlready);
        let off = json!([{ "id": "allow_all", "currentValue": "off" }]);
        let s = statement(
            "GitHub Copilot",
            Some(copilot),
            Some(&a),
            &plan,
            None,
            None,
            &off,
        );
        assert_eq!(s.asks(), Some(true));
        assert_eq!(s.sentence(), "GitHub Copilot asks before acting: Agent");
        // Allow-all on: whatever the mode, it decides alone.
        let on = json!([{ "id": "allow_all", "currentValue": "on" }]);
        let s = statement(
            "GitHub Copilot",
            Some(copilot),
            Some(&a),
            &plan,
            None,
            None,
            &on,
        );
        assert_eq!(s.asks(), Some(false));
        assert_eq!(
            s.sentence(),
            "GitHub Copilot is in Agent — decides without asking: everything (allow_all is on)"
        );
        // Codex asked for read-only; one that did not answer stays deciding.
        let codex = profile("codex");
        let c = ad("agent", CODEX);
        let plan = ModePlan::of(Some(codex), Some(&c), None);
        let s = statement(
            "Codex",
            Some(codex),
            Some(&c),
            &plan,
            Some(&Outcome::NoAnswer),
            None,
            &json!([]),
        );
        assert_eq!(
            s.sentence(),
            "asked Codex to ask before acting; it did not answer — it is in agent — decides without asking: a reviewer model decides"
        );
        // OpenCode asks through its launch rule, unless a mode was chosen.
        let open = profile("opencode");
        let o = ad("build", &["build", "plan"]);
        let plan = ModePlan::of(Some(open), Some(&o), None);
        assert_eq!(
            statement(
                "OpenCode",
                Some(open),
                Some(&o),
                &plan,
                None,
                None,
                &json!([])
            )
            .asks(),
            Some(true)
        );
        let plan = ModePlan::of(Some(open), Some(&o), Some("build"));
        assert_eq!(
            statement(
                "OpenCode",
                Some(open),
                Some(&o),
                &plan,
                None,
                Some("build"),
                &json!([])
            )
            .asks(),
            None
        );
    }

    /// Every value a profile names is in its adapter's recorded set-up.
    #[test]
    fn every_profile_value_is_in_its_recording() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/acp");
        for p in PROFILES {
            let file = root.join(format!("{}@{}.json", p.agent, p.agent_info.1));
            let text = std::fs::read_to_string(&file)
                .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
            let rec: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            let info = &rec["initialize"]["agentInfo"];
            assert_eq!(
                (info["name"].as_str(), info["version"].as_str()),
                (Some(p.agent_info.0), Some(p.agent_info.1)),
                "{}",
                p.agent
            );
            let setup = &rec["session/new"];
            let a = Advertised::read(&setup["configOptions"], &setup["modes"], Some(p))
                .unwrap_or_else(|| panic!("{}: no mode advertised", p.agent));
            assert!(p.offers_exactly(&a), "{}: {:?}", p.agent, a.values);
            assert_eq!(a.current, p.unconfigured_start, "{}", p.agent);
            if let Some((id, asks)) = p.guard {
                assert_eq!(
                    current_of(&setup["configOptions"], id).as_deref(),
                    Some(asks)
                );
            }
        }
    }
}
