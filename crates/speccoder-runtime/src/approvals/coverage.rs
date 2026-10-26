//! What a prefix entry would newly allow, from the project's journal: the
//! preview a prefix is saved from (`[commands] allow_prefix`, `[sandbox]
//! network_allow_prefix`). Every distinct argv the journal records that the
//! prefix covers and today's policy does not allow, with how often, when
//! last and how it was decided then; what a fixed rule still asks for or
//! denies is marked. An unreadable journal or run makes the preview
//! incomplete — never "covers nothing" — and an incomplete preview cannot
//! be saved.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use speccoder_core::actions::{PolicyList, Via};
use speccoder_core::{
    CommandData, CommandResult, CommandStatus, ConfigChange, CoverageDecision, CoverageEntry,
    CoverageRow, PolicyCoverage,
};

use crate::tools::policy::{Decision, Policy, prefix_matches, prefix_refusal};

/// The policy the committed `speccoder.toml` at `root` gives: the checks'
/// prefixes and every allow list (what is in force).
fn policy_now(root: &Path) -> Policy {
    let dir = crate::verify::find_config(root).unwrap_or_else(|| root.to_path_buf());
    let committed = crate::git::Git::at(&dir)
        .run(&["show", &format!("HEAD:./{}", crate::verify::config::FILE)])
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .or_else(|| std::fs::read_to_string(dir.join(crate::verify::config::FILE)).ok())
        .unwrap_or_default();
    let checks: Vec<Vec<String>> = crate::verify::config::parse(&committed)
        .map(|cs| cs.into_iter().map(|c| c.argv).collect())
        .unwrap_or_default();
    let run = crate::substrate::run_config::parse(&committed).unwrap_or_default();
    let sandbox = crate::sandbox::parse(&committed).unwrap_or_default();
    Policy::new(&checks, &run.allow)
        .with_prefixes(run.allow_prefix)
        .with_network(sandbox.network_allow)
        .with_network_prefixes(sandbox.network_allow_prefix)
}

/// Whether `policy` lets `argv` run as asked: without the network (the
/// commands list) or with it (the network list).
fn allowed(policy: &Policy, list: PolicyList, argv: &[String]) -> Decision {
    let args = match list {
        PolicyList::CommandsPrefix => json!({ "argv": argv }),
        PolicyList::NetworkPrefix => json!({ "argv": argv, "network": true }),
    };
    policy.base("run_command", &args)
}

/// One argv's calls: how many, the latest, and how decided.
type Tally = (u32, Option<u64>, BTreeMap<String, u32>);

/// The preview of adding `prefix` to `list`, for the project at `root`
/// whose state lives under `state`.
#[must_use]
pub fn preview(root: &Path, state: &Path, list: PolicyList, prefix: &[String]) -> PolicyCoverage {
    let project = crate::format::at(root)
        .detect(root)
        .unwrap_or_else(|| root.to_path_buf());
    let dir = crate::worktree::project_state_dir(state, &project);
    let now = policy_now(&project);
    let widened = match list {
        PolicyList::CommandsPrefix => now.clone().with_prefixes(vec![prefix.to_vec()]),
        PolicyList::NetworkPrefix => now.clone().with_network_prefixes(vec![prefix.to_vec()]),
    };
    let mut unreadable = Vec::new();
    // argv → (count, last, decided)
    let mut rows: BTreeMap<Vec<String>, Tally> = BTreeMap::new();
    if dir.join(crate::substrate::JOURNAL).exists() {
        match read_calls(&dir) {
            Ok(recorded) => {
                unreadable.extend(recorded.unreadable);
                for call in recorded.calls {
                    let network = call.arguments["network"] == true;
                    if network != (list == PolicyList::NetworkPrefix) {
                        continue;
                    }
                    let argv = crate::tools::policy::argv_of(&call.arguments);
                    if !prefix_matches(prefix, &argv)
                        || allowed(&now, list, &argv) == Decision::Allow
                    {
                        continue;
                    }
                    let how = if call.arguments.get(crate::tools::EXPECT).is_some() {
                        "approved by a person"
                    } else {
                        "allowed by rule"
                    };
                    let row = rows.entry(argv).or_default();
                    row.0 += 1;
                    row.1 = row.1.max(Some(call.at));
                    *row.2.entry(how.to_owned()).or_default() += 1;
                }
            }
            Err(e) => unreadable.push(format!("the journal: {e}")),
        }
    }
    let rows: Vec<CoverageRow> = rows
        .into_iter()
        .map(|(argv, (count, last_at, decided))| CoverageRow {
            still_asks: match allowed(&widened, list, &argv) {
                Decision::Allow => None,
                Decision::Approval(r) => Some(format!("still asks: {r}")),
                Decision::Deny(r) => Some(format!("still denied: {r}")),
            },
            argv,
            count,
            last_at,
            decided: decided
                .into_iter()
                .map(|(decision, count)| CoverageDecision { decision, count })
                .collect(),
        })
        .collect();
    let entry = CoverageEntry {
        key: list.key().to_owned(),
        prefix: prefix.to_vec(),
    };
    let complete = unreadable.is_empty();
    let digest = digest(&entry, &rows, complete);
    PolicyCoverage {
        entry,
        complete,
        unreadable,
        rows,
        digest,
    }
}

fn read_calls(dir: &Path) -> Result<crate::substrate::RecordedCalls, String> {
    let rt = crate::tokio_runtime()?;
    let Some(plane) = crate::substrate::Plane::open_for_reading(dir).map_err(|e| e.0)? else {
        return Ok(crate::substrate::RecordedCalls::default());
    };
    rt.block_on(plane.tool_calls("proc", "run_command"))
        .map_err(|e| e.0)
}

/// SHA-256 over the canonical JSON of the entry, the rows and `complete`.
#[must_use]
pub fn digest(entry: &CoverageEntry, rows: &[CoverageRow], complete: bool) -> String {
    let value = json!({ "entry": entry, "rows": rows, "complete": complete });
    let text = canonical(&value);
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// JSON with object keys sorted (`serde_json` keeps insertion order).
fn canonical(v: &Value) -> String {
    match v {
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let inner: Vec<String> = keys
                .into_iter()
                .map(|k| format!("{}:{}", Value::String(k.clone()), canonical(&o[k])))
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        other => other.to_string(),
    }
}

/// `speccoder config coverage`: the preview as a result.
#[must_use]
pub fn show(
    root: &Path,
    state: Option<&Path>,
    list: PolicyList,
    prefix: &[String],
) -> CommandResult {
    let Some(state) = state else {
        return CommandResult::new("config", CommandStatus::ConfigError, "no state directory")
            .with_code("NO-STATE-DIR");
    };
    if let Some(why) = prefix_refusal(prefix) {
        return CommandResult::new(
            "config",
            CommandStatus::UsageError,
            &format!("`{}`: {why}", list.key()),
        );
    }
    let c = preview(root, state, list, prefix);
    let message = summary(&c);
    CommandResult::new("config", CommandStatus::Ok, &message)
        .with_data(CommandData::PolicyCoverage(Box::new(c)))
}

/// One line: what it would newly allow, or why the preview is incomplete.
#[must_use]
pub fn summary(c: &PolicyCoverage) -> String {
    let what = format!(
        "`{}` += {}: {} command(s) from the journal would run without asking",
        c.entry.key,
        serde_json::to_string(&c.entry.prefix).unwrap_or_default(),
        c.rows.iter().filter(|r| r.still_asks.is_none()).count()
    );
    if c.complete {
        what
    } else {
        format!(
            "{what} — the preview is incomplete ({}); saving is refused until it can be read",
            c.unreadable.join("; ")
        )
    }
}

/// `WidenPolicy`: adds `prefix` to `list` as the confirmation of the
/// preview whose digest is `digest` — refused for a bare program, an
/// incomplete preview, or one that changed since it was shown.
#[must_use]
pub fn widen(
    root: &Path,
    state: Option<&Path>,
    list: PolicyList,
    prefix: &[String],
    digest: &str,
    via: Via,
) -> CommandResult {
    let refuse = |m: String| CommandResult::new("config", CommandStatus::UsageError, &m);
    let Some(state) = state else {
        return CommandResult::new("config", CommandStatus::ConfigError, "no state directory")
            .with_code("NO-STATE-DIR");
    };
    if let Some(why) = prefix_refusal(prefix) {
        return refuse(format!("`{}`: {why}", list.key()));
    }
    let c = preview(root, state, list, prefix);
    if !c.complete {
        return refuse(format!(
            "the preview is incomplete: {}; try again when the journal can be read",
            c.unreadable.join("; ")
        ))
        .with_data(CommandData::PolicyCoverage(Box::new(c)));
    }
    if c.digest != digest {
        return refuse("the preview changed; review it again".to_owned())
            .with_data(CommandData::PolicyCoverage(Box::new(c)));
    }
    let r = crate::settings::apply_confirmed(
        root,
        &[ConfigChange {
            key: list.key().to_owned(),
            value: None,
            append: Some(json!(prefix)),
        }],
    );
    if r.status() != CommandStatus::Ok {
        return r;
    }
    let by = crate::approvals::Who::local_in(root, via);
    CommandResult::new(
        "config",
        CommandStatus::Ok,
        &format!(
            "added {} to `{}` from its preview ({} command(s) newly allowed) — by {} ({}); commit speccoder.toml to put it in force",
            serde_json::to_string(prefix).unwrap_or_default(),
            list.key(),
            c.rows.iter().filter(|r| r.still_asks.is_none()).count(),
            by.name,
            by.via
        ),
    )
    .with_next(r.next().to_vec())
    .with_data(r.data().cloned().unwrap_or(CommandData::PolicyCoverage(Box::new(c))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_covers_the_rows_and_completeness() {
        let entry = CoverageEntry {
            key: "commands.allow_prefix".to_owned(),
            prefix: vec!["npm".to_owned(), "run".to_owned()],
        };
        let row = CoverageRow {
            argv: vec!["npm".to_owned(), "run".to_owned(), "build".to_owned()],
            count: 1,
            last_at: Some(1),
            decided: Vec::new(),
            still_asks: None,
        };
        let empty = digest(&entry, &[], true);
        assert_ne!(empty, digest(&entry, std::slice::from_ref(&row), true));
        assert_ne!(empty, digest(&entry, &[], false));
        assert_eq!(empty, digest(&entry, &[], true));
    }
}
