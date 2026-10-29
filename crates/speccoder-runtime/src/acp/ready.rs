//! How ready each agent is to make attempts: installed, signed in, asking
//! before it acts — each with its one action (an install command of the
//! person's own, the agent's login in a terminal of theirs, or the reason).
//!
//! An installed agent is probed: started, `initialize`d and given one
//! session in a scratch directory, never a prompt — so it costs nothing
//! and asks nothing. An `npx` preset is not started: that would download
//! it; its ticks say what is known without it.

use std::path::Path;
use std::time::Duration;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    CloseSessionRequest, InitializeRequest, NewSessionRequest,
};
use agent_client_protocol::{AcpAgent, AcpAgentConfig, Agent as AgentRole, ConnectionTo};
use speccoder_core::{AgentCard, AgentTick, TickState};

use super::agent::{Agent, PRESETS, Preset};
use super::modes::{self, Advertised, ModePlan, Why};

/// How long a probe may take.
pub const PROBE: Duration = Duration::from_secs(20);

/// What a probe found.
#[derive(Debug, Clone, Default)]
pub struct Probed {
    /// The agent's name and version, as it gave them.
    pub info: Option<(String, String)>,
    /// It needs signing in: its ways.
    pub sign_in: Option<Vec<super::session::SignIn>>,
    /// Why the probe failed otherwise.
    pub error: Option<String>,
    /// What speccoder would do about its mode, and what it would read.
    pub mode: Option<(
        ModePlan,
        Option<&'static modes::ModeProfile>,
        Option<Advertised>,
    )>,
}

/// Starts `argv` in a scratch directory, opens one session and closes it.
#[must_use]
pub fn probe(agent: &Agent, argv: &[String]) -> Probed {
    let Ok(dir) = tempfile::tempdir() else {
        return Probed {
            error: Some("no scratch directory".to_owned()),
            ..Probed::default()
        };
    };
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return Probed {
            error: Some("the runtime could not start".to_owned()),
            ..Probed::default()
        };
    };
    let Some((program, args)) = argv.split_first() else {
        return Probed::default();
    };
    let mut config = AcpAgentConfig::new(program).args(args.iter().cloned());
    if agent.mode.is_none()
        && let Some((k, v)) = modes::launch_profile(&agent.id).and_then(|p| p.launch_env)
    {
        config = config.env(k, v);
    }
    let cwd = std::fs::canonicalize(dir.path()).unwrap_or_else(|_| dir.path().to_path_buf());
    let started = AcpAgent::new(super::session::in_directory(config, &cwd));
    let out = std::sync::Arc::new(std::sync::Mutex::new(Probed::default()));
    let result = std::sync::Arc::clone(&out);
    let preference = agent.mode.clone();
    let argv = argv.to_vec();
    let ran = rt.block_on(async move {
        tokio::time::timeout(
            PROBE,
            agent_client_protocol::Client
                .builder()
                .name("speccoder")
                .connect_with(started, move |c: ConnectionTo<AgentRole>| async move {
                    let p = open(&c, &cwd, preference.as_deref(), &argv).await;
                    if let Ok(mut r) = result.lock() {
                        *r = p;
                    }
                    Ok(())
                }),
        )
        .await
    });
    let mut p = out.lock().map(|p| p.clone()).unwrap_or_default();
    match ran {
        Err(_) if p.info.is_none() => {
            p.error = Some(format!("it did not answer within {} s", PROBE.as_secs()));
        }
        Ok(Err(e)) if p.info.is_none() && p.error.is_none() => p.error = Some(e.to_string()),
        _ => {}
    }
    p
}

async fn open(
    c: &ConnectionTo<AgentRole>,
    cwd: &Path,
    preference: Option<&str>,
    argv: &[String],
) -> Probed {
    let mut p = Probed::default();
    let init = match c
        .send_request(
            InitializeRequest::new(ProtocolVersion::V1)
                .client_capabilities(super::session::declared()),
        )
        .block_task()
        .await
    {
        Ok(i) => i,
        Err(e) => {
            p.error = Some(format!("it did not initialize: {}", e.message));
            return p;
        }
    };
    p.info = init
        .agent_info
        .as_ref()
        .map(|i| (i.name.clone(), i.version.clone()));
    let methods = serde_json::to_value(&init.auth_methods).unwrap_or_default();
    let new = match c
        .send_request(NewSessionRequest::new(cwd.to_path_buf()))
        .block_task()
        .await
    {
        Ok(n) => n,
        Err(e) => {
            if e.code == agent_client_protocol::ErrorCode::AuthRequired
                || !init.auth_methods.is_empty()
            {
                p.sign_in = Some(super::session::sign_in_methods(&methods, argv));
            }
            p.error = Some(e.message);
            return p;
        }
    };
    let setup = serde_json::to_value(&new).unwrap_or_default();
    let profile = p
        .info
        .as_ref()
        .and_then(|(n, v)| modes::profile_for(n, v, &setup["configOptions"], &setup["modes"]));
    let advertised = Advertised::read(&setup["configOptions"], &setup["modes"], profile);
    p.mode = Some((
        ModePlan::of(profile, advertised.as_ref(), preference),
        profile,
        advertised,
    ));
    if init.agent_capabilities.session_capabilities.close.is_some() {
        let _ = tokio::time::timeout(
            Duration::from_secs(2),
            c.send_request(CloseSessionRequest::new(new.session_id))
                .block_task(),
        )
        .await;
    }
    p
}

/// The install command for a preset, where there is one this machine can
/// run.
fn install(p: &Preset, on_path: &dyn Fn(&str) -> bool) -> AgentTick {
    let mac = cfg!(target_os = "macos") && on_path("brew");
    let s = |a: &[&str]| vec![a.iter().map(|w| (*w).to_owned()).collect::<Vec<String>>()];
    let missing = AgentTick::new(TickState::No, format!("`{}` is not on PATH", p.argv[0]));
    match p.id {
        "copilot" if mac => missing.with_command(s(&["brew", "install", "--cask", "copilot-cli"])),
        "copilot" if on_path("npm") => {
            missing.with_command(s(&["npm", "install", "-g", "@github/copilot"]))
        }
        "opencode" if mac => missing.with_command(s(&["brew", "install", "opencode"])),
        "opencode" if on_path("npm") => {
            missing.with_command(s(&["npm", "install", "-g", "opencode-ai"]))
        }
        _ if p.argv[0] == "npx" && mac => {
            AgentTick::new(TickState::No, "it runs through `npx`, which needs Node.js")
                .with_command(s(&["brew", "install", "node"]))
        }
        _ if p.argv[0] == "npx" => {
            AgentTick::new(TickState::No, "it runs through `npx`, which needs Node.js")
                .with_words("install Node.js (https://nodejs.org), then come back")
        }
        _ => missing.with_words(format!(
            "install {} and put `{}` on PATH",
            p.name, p.argv[0]
        )),
    }
}

/// One preset's card; `probe` starts an installed one.
#[must_use]
pub fn card(
    p: &Preset,
    chosen: Option<&Agent>,
    on_path: &dyn Fn(&str) -> bool,
    probe_it: bool,
) -> AgentCard {
    let agent = match chosen.filter(|a| a.id == p.id) {
        Some(a) => a.clone(),
        None => Agent::of(p, None),
    };
    let argv = agent.command(|x| on_path(x));
    let npx = argv.first().is_some_and(|a| a == "npx");
    let pin = Agent::npm_pin(&argv).map(|(n, v)| format!("{n}@{v}"));
    let runs = argv.first().is_some_and(|a| on_path(a));
    let installed = if runs {
        match &pin {
            Some(pin) => AgentTick::new(
                TickState::Yes,
                format!("through npx: {pin}, downloaded when it first runs"),
            ),
            None => AgentTick::new(TickState::Yes, format!("`{}` on PATH", argv.join(" "))),
        }
    } else {
        install(p, on_path)
    };
    let login =
        || -> Vec<Vec<String>> { vec![p.login.split_whitespace().map(str::to_owned).collect()] };
    let mut signed_in = AgentTick::new(TickState::Unknown, "checked when it first runs");
    let mut asks_first = recorded(p, pin.as_deref());
    if runs && !npx && probe_it {
        let probed = probe(&agent, &argv);
        let version = probed.info.as_ref().map(|(_, v)| v.clone());
        signed_in = match (&probed.sign_in, &probed.error) {
            (Some(ways), _) => {
                let mut t = AgentTick::new(
                    TickState::No,
                    ways.iter()
                        .find_map(|w| w.description.clone())
                        .unwrap_or_else(|| format!("{} needs signing in", p.name)),
                );
                let commands: Vec<Vec<String>> = ways
                    .iter()
                    .filter_map(|w| w.command.as_deref())
                    .map(|c| c.split_whitespace().map(str::to_owned).collect())
                    .collect();
                for c in if commands.is_empty() {
                    login()
                } else {
                    commands
                } {
                    t = t.with_command(vec![c]);
                }
                t
            }
            (None, Some(e)) => {
                AgentTick::new(TickState::Unknown, format!("it could not be asked: {e}"))
            }
            (None, None) => AgentTick::new(
                TickState::Yes,
                format!(
                    "it opened a session{}",
                    version
                        .as_deref()
                        .map(|v| format!(" ({v})"))
                        .unwrap_or_default()
                ),
            ),
        };
        if let Some((plan, profile, advertised)) = &probed.mode {
            asks_first = asks(p, plan, *profile, advertised.as_ref());
        }
    } else if runs && !npx {
        signed_in = AgentTick::new(TickState::Unknown, "not checked (doctor --agents asks it)");
    } else if npx {
        signed_in = AgentTick::new(
            TickState::Unknown,
            "checked when it first runs: npx downloads it then",
        );
    }
    AgentCard {
        id: p.id.to_owned(),
        name: p.name.to_owned(),
        chosen: chosen.is_some_and(|a| a.id == p.id),
        installed,
        signed_in,
        asks_first,
    }
}

/// The asks-first tick from a live set-up answer.
fn asks(
    p: &Preset,
    plan: &ModePlan,
    profile: Option<&modes::ModeProfile>,
    advertised: Option<&Advertised>,
) -> AgentTick {
    let name = |v: &str| advertised.map_or_else(|| v.to_owned(), |a| a.name_of(v));
    match (plan.why, &plan.request) {
        (Why::AskedForAskFirst, Some(v)) => AgentTick::new(
            TickState::Yes,
            format!("speccoder puts it into {} before each attempt", name(v)),
        ),
        (Why::AsksAlready, _) if profile.is_some_and(|p| p.launch_env.is_some()) => AgentTick::new(
            TickState::Yes,
            "speccoder starts it with every permission rule set to ask",
        ),
        (Why::AsksAlready, _) => AgentTick::new(
            TickState::Yes,
            format!(
                "it asks first: {}",
                advertised.map_or_else(String::new, |a| a.name_of(&a.current))
            ),
        ),
        (Why::YouChose, _) => {
            AgentTick::new(TickState::Unknown, "you chose its mode ([executor] mode)")
        }
        (Why::YouSetIt, _) => AgentTick::new(
            TickState::No,
            format!(
                "its own settings put it in {}, which speccoder keeps: it decides some calls alone",
                advertised.map_or_else(|| "another mode".to_owned(), |a| a.name_of(&a.current))
            ),
        )
        .with_words(format!(
            "set {} back to its ask-first mode in its own settings",
            p.name
        )),
        (Why::Unprofiled, _) => AgentTick::new(
            TickState::Unknown,
            "no recording of this version says which mode asks first: it may decide alone",
        ),
        _ => AgentTick::new(
            TickState::No,
            "it offers no mode known to ask first: it may decide alone",
        ),
    }
}

/// The asks-first tick from the recordings alone.
fn recorded(p: &Preset, pin: Option<&str>) -> AgentTick {
    let Some(profile) = modes::launch_profile(p.id) else {
        return AgentTick::new(
            TickState::Unknown,
            "no recording says which mode asks first",
        );
    };
    let version = pin.and_then(|x| x.rsplit('@').next());
    match version {
        Some(v) if v != profile.agent_info.1 => AgentTick::new(
            TickState::Unknown,
            format!(
                "recorded for {}; {v} is checked against it when it runs",
                profile.agent_info.1
            ),
        ),
        _ => AgentTick::new(
            TickState::Yes,
            format!(
                "recorded for {} {}",
                profile.agent_info.0, profile.agent_info.1
            ),
        ),
    }
}

/// Every preset's card; installed ones probed (in parallel) when `probe`.
#[must_use]
pub fn cards(chosen: Option<&Agent>, probe_them: bool) -> Vec<AgentCard> {
    let on_path = |x: &str| super::agent::on_path(x);
    std::thread::scope(|s| {
        let handles: Vec<_> = PRESETS
            .iter()
            .map(|p| s.spawn(move || card(p, chosen, &on_path, probe_them)))
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    })
}

/// The next agent to try after `current`: the first ready-looking preset
/// after it (installed; not known to need a sign-in), in preset order.
#[must_use]
pub fn next_after(
    current: Option<&str>,
    on_path: &dyn Fn(&str) -> bool,
) -> Option<&'static Preset> {
    let start = current
        .and_then(|c| PRESETS.iter().position(|p| p.id == c))
        .map_or(0, |i| i + 1);
    PRESETS
        .iter()
        .cycle()
        .skip(start)
        .take(PRESETS.len())
        .filter(|p| Some(p.id) != current)
        .find(|p| {
            Agent::of(p, None)
                .command(on_path)
                .first()
                .is_some_and(|a| on_path(a))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(id: &str) -> &'static Preset {
        super::super::agent::preset(id).unwrap_or(&PRESETS[0])
    }

    #[test]
    fn a_missing_agent_has_its_install_command_and_an_npx_one_is_not_started() {
        let none = |_: &str| false;
        let c = card(preset("opencode"), None, &none, true);
        assert_eq!(c.installed.state, TickState::No);
        assert!(!c.installed.fix.is_empty(), "{:?}", c.installed);
        // With npm (and no Homebrew): its install, run as the person's own
        // command, outside the project — and said so.
        let npm = |x: &str| x == "npm";
        let c = card(preset("opencode"), None, &npm, true);
        let shown = serde_json::to_value(&c.installed).unwrap_or_default();
        let step = &shown["fix_steps"][0];
        assert_eq!(
            step["argvs"],
            serde_json::json!([["npm", "install", "-g", "opencode-ai"]]),
            "{shown}"
        );
        assert_eq!(step["how"], "command", "{shown}");
        assert!(
            step["outside"]
                .as_str()
                .unwrap_or_default()
                .starts_with("installs "),
            "{shown}"
        );
        let npx = |x: &str| x == "npx";
        let c = card(preset("claude"), None, &npx, true);
        assert_eq!(c.installed.state, TickState::Yes);
        assert!(
            c.installed
                .detail
                .contains("@agentclientprotocol/claude-agent-acp@0.98.0")
        );
        assert_eq!(c.signed_in.state, TickState::Unknown);
        // Recorded for another version than pinned: not known until it runs.
        assert_eq!(c.asks_first.state, TickState::Unknown);
        let c = card(preset("copilot"), None, &npx, false);
        assert_eq!(c.asks_first.state, TickState::Yes, "{:?}", c.asks_first);
    }

    #[test]
    fn the_next_agent_is_the_next_installed_one() {
        let have = |x: &str| x == "opencode" || x == "copilot";
        assert_eq!(
            next_after(Some("opencode"), &have).map(|p| p.id),
            Some("copilot")
        );
        let npx = |x: &str| x == "opencode" || x == "npx";
        assert_eq!(
            next_after(Some("opencode"), &npx).map(|p| p.id),
            Some("claude")
        );
        let only = |x: &str| x == "opencode";
        assert_eq!(next_after(Some("opencode"), &only), None);
        assert_eq!(next_after(None, &only).map(|p| p.id), Some("opencode"));
    }
}
