//! `speccoder.toml` as settings: a catalogue of every key — its
//! type, default and meaning — the values a project sets, and edits that
//! keep the file's comments and are checked by every parser before they are
//! written. speccoder reads committed configuration only, so a change is
//! offered as a commit of exactly this file.

use std::path::{Path, PathBuf};

use serde_json::Value;
use speccoder_core::{
    CommandData, CommandResult, CommandStatus, ConfigChange, ConfigKey, ConfigReport,
    ConfigSection, ConfigTable,
};

use crate::git::Git;
use crate::verify::config::FILE;

mod catalogue;
mod edit;
#[cfg(test)]
mod tests;

pub use catalogue::{CATALOGUE, Key, Kind, Section};
pub use edit::{edit, value_of};

/// Whether every parser accepts `text` — the same ones every command uses.
///
/// # Errors
///
/// The first parser's complaint.
pub fn validate(text: &str) -> Result<(), String> {
    crate::substrate::model_config::parse(text)?;
    validate_but_model(text)
}

/// [`validate`] without `[model]`.
pub(super) fn validate_but_model(text: &str) -> Result<(), String> {
    crate::verify::config::parse(text)?;
    crate::substrate::run_config::parse(text)?;
    crate::sandbox::parse(text)?;
    crate::prepare::parse(text)?;
    crate::mcp::parse(text)?;
    crate::mutation::parse(text)?;
    crate::external::parse(text)?;
    crate::format::parse_config(text)?;
    Ok(())
}

/// The directory holding the configuration: the nearest with one, else the
/// repository's top level, else `root`.
pub(super) fn home(root: &Path) -> PathBuf {
    // Where the file is, else where `init` would write it (the spec
    // project's root, which in a nested project is not the repository's).
    crate::verify::find_config(root).unwrap_or_else(|| crate::init::root_of(root))
}

pub(super) fn lookup<'a>(table: &'a toml::Table, path: &[&str]) -> Option<&'a toml::Value> {
    let (last, parents) = path.split_last()?;
    let mut t = table;
    for p in parents {
        t = t.get(*p)?.as_table()?;
    }
    t.get(*last)
}

pub(super) fn key_view(k: &Key, value: Option<Value>) -> ConfigKey {
    let (min, max) = match k.kind {
        #[allow(clippy::cast_precision_loss)]
        Kind::Int(a, b) => (Some(a as f64), Some(b as f64)),
        Kind::Num(a, b) => (Some(a), Some(b)),
        _ => (None, None),
    };
    ConfigKey {
        key: k.key.to_owned(),
        kind: k.kind.name().to_owned(),
        about: k.about.to_owned(),
        default: k.default.and_then(|d| serde_json::from_str(d).ok()),
        value,
        choices: match k.kind {
            Kind::Choice(c) => c.iter().map(|s| (*s).to_owned()).collect(),
            _ => Vec::new(),
        },
        min,
        max,
        required: k.required,
    }
}

pub(super) fn json_of(v: &toml::Value) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// The configuration as settings: every key, set or default.
#[must_use]
pub fn read(root: &Path) -> ConfigReport {
    let dir = home(root);
    let path = dir.join(FILE);
    let text = std::fs::read_to_string(&path).ok();
    let table: toml::Table = text
        .as_deref()
        .and_then(|t| toml::from_str(t).ok())
        .unwrap_or_default();
    let sections = CATALOGUE
        .iter()
        .map(|s| {
            if s.named {
                let tables = table
                    .get(s.name)
                    .and_then(toml::Value::as_table)
                    .map(|named| {
                        named
                            .iter()
                            .map(|(name, t)| ConfigTable {
                                name: name.clone(),
                                keys: s
                                    .keys
                                    .iter()
                                    .map(|k| {
                                        let v =
                                            t.as_table().and_then(|t| t.get(k.key)).map(json_of);
                                        key_view(k, v)
                                    })
                                    .collect(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                ConfigSection {
                    name: s.name.to_owned(),
                    title: s.title.to_owned(),
                    about: s.about.to_owned(),
                    named: true,
                    keys: s.keys.iter().map(|k| key_view(k, None)).collect(),
                    tables,
                }
            } else {
                ConfigSection {
                    name: s.name.to_owned(),
                    title: s.title.to_owned(),
                    about: s.about.to_owned(),
                    named: false,
                    keys: s
                        .keys
                        .iter()
                        .map(|k| {
                            let mut path = vec![s.name];
                            path.extend(k.key.split('.'));
                            let mut view = key_view(k, lookup(&table, &path).map(json_of));
                            // Bedrock's region is required with that provider.
                            view.required |= s.name == "model"
                                && k.key == "region"
                                && lookup(&table, &["model", "provider"])
                                    .and_then(toml::Value::as_str)
                                    == Some("bedrock");
                            view
                        })
                        .collect(),
                    tables: Vec::new(),
                }
            }
        })
        .collect();
    let error = text.as_deref().and_then(|t| validate(t).err());
    let (committed, diff) = state(&dir, text.is_some());
    ConfigReport {
        file: FILE.to_owned(),
        exists: text.is_some(),
        committed,
        valid: text.is_some() && error.is_none(),
        error,
        sections,
        diff,
    }
}

/// Whether the file on disk is the committed one, and the change if not.
pub(super) fn state(dir: &Path, exists: bool) -> (bool, Option<String>) {
    let g = Git::at(dir);
    let tracked = g
        .succeeds(&["ls-files", "--error-unmatch", "--", FILE])
        .unwrap_or(false);
    if !tracked {
        let diff = exists.then(|| {
            std::fs::read_to_string(dir.join(FILE))
                .unwrap_or_default()
                .lines()
                .map(|l| format!("+{l}"))
                .collect::<Vec<_>>()
                .join("\n")
        });
        return (
            false,
            diff.map(|d| {
                format!("diff --git a/{FILE} b/{FILE}\nnew file mode 100644\n@@ -0,0 +1 @@\n{d}\n")
            }),
        );
    }
    let clean = g
        .succeeds(&["diff", "--quiet", "HEAD", "--", FILE])
        .unwrap_or(false);
    if clean {
        return (true, None);
    }
    let diff = g
        .run(&["diff", "--no-color", "HEAD", "--", FILE])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .ok();
    (false, diff)
}

/// What a change to `[model]` leaves to do: Bedrock's region when it is
/// still missing (the step [`edit`] lets through), and a warning when a
/// change made the model a Bedrock one with an id Bedrock does not use.
fn model_notes(
    text: &str,
    changes: &[ConfigChange],
) -> (Vec<String>, Vec<speccoder_core::next::NextStep>) {
    use crate::substrate::model_config;
    let mut notes = Vec::new();
    let mut next = Vec::new();
    if model_config::parse(text).is_err_and(|e| e.starts_with(model_config::NEEDS_REGION)) {
        notes.push("provider bedrock needs model.region, set it next".to_owned());
        next.push(speccoder_core::next::NextStep {
            command: "speccoder config set model.region <region>".to_owned(),
            why: "provider bedrock needs the AWS region its calls go to".to_owned(),
        });
    }
    let touched = changes
        .iter()
        .any(|c| c.key == "model.provider" || c.key == "model.model");
    let model: toml::Table = toml::from_str(text).unwrap_or_default();
    let model = model.get("model").and_then(toml::Value::as_table);
    let field = |k: &str| model.and_then(|m| m.get(k)).and_then(toml::Value::as_str);
    if touched
        && field("provider") == Some("bedrock")
        && let Some(w) = field("model").and_then(model_config::bedrock_model_warning)
    {
        notes.push(format!("warning: {w}"));
    }
    (notes, next)
}

/// `speccoder config`: the settings.
#[must_use]
pub fn show(root: &Path) -> CommandResult {
    let report = read(root);
    let message = match (&report.exists, &report.error, report.committed) {
        (false, _, _) => "no speccoder.toml yet: every key has its default".to_owned(),
        (true, Some(e), _) => format!("speccoder.toml has an error: {e}"),
        (true, None, false) => {
            "speccoder.toml has changes that are not committed (not in force yet)".to_owned()
        }
        (true, None, true) => "speccoder.toml".to_owned(),
    };
    let result = CommandResult::new("config", CommandStatus::Ok, &message);
    let result = if report.committed || !report.exists {
        result
    } else {
        result.with_next(vec![speccoder_core::next::NextStep {
            command: "speccoder config commit".to_owned(),
            why: "speccoder reads committed configuration only".to_owned(),
        }])
    };
    result.with_data(CommandData::Config(Box::new(report)))
}

/// The keys only a coverage preview's confirmation writes.
pub const PREFIX_KEYS: [&str; 2] = ["commands.allow_prefix", "sandbox.network_allow_prefix"];

/// `speccoder config set|unset`, and the page's save: applies `changes` to
/// the file, all or nothing. A prefix key is refused: it is saved only as
/// the confirmation of its coverage preview (`WidenPolicy`); removing an
/// entry (`unset`) narrows and is allowed.
#[must_use]
pub fn apply(root: &Path, changes: &[ConfigChange]) -> CommandResult {
    if let Some(c) = changes.iter().find(|c| {
        PREFIX_KEYS.contains(&c.key.as_str()) && (c.value.is_some() || c.append.is_some())
    }) {
        // Setting the list to fewer entries than it has narrows it.
        let narrows = c.value.as_ref().is_some_and(|v| {
            let now = read(root);
            let current = now
                .sections
                .iter()
                .flat_map(|s| {
                    s.keys
                        .iter()
                        .map(move |k| (format!("{}.{}", s.name, k.key), k))
                })
                .find(|(name, _)| *name == c.key)
                .and_then(|(_, k)| k.value.clone())
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
            v.as_array()
                .is_some_and(|new| new.iter().all(|e| current.contains(e)))
        });
        if !narrows {
            return CommandResult::new(
                "config",
                CommandStatus::UsageError,
                &format!(
                    "`{}` is saved only from its coverage preview: `speccoder config coverage {} <argv…>`, then confirm",
                    c.key, c.key
                ),
            )
            .with_code("CONFIG-INVALID");
        }
    }
    apply_confirmed(root, changes)
}

/// [`apply`] without the prefix guard: for the confirmation of a coverage
/// preview and for Always Allow's exact entry, which their actions checked.
#[must_use]
pub(crate) fn apply_confirmed(root: &Path, changes: &[ConfigChange]) -> CommandResult {
    let dir = home(root);
    let path = dir.join(FILE);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    match edit(&text, changes) {
        Err(e) => CommandResult::new("config", CommandStatus::ConfigError, &e)
            .with_code("CONFIG-INVALID")
            .with_fix(vec![
                crate::substrate::model_config::config_fix(&e).unwrap_or_else(|| {
                    format!(
                        "speccoder config get {}",
                        changes.first().map_or("", |c| c.key.as_str())
                    )
                }),
            ]),
        Ok(out) if out == text => show(root),
        Ok(out) => match std::fs::write(&path, &out) {
            Ok(()) => {
                let keys: Vec<&str> = changes.iter().map(|c| c.key.as_str()).collect();
                let r = show(root);
                let (notes, mut next) = model_notes(&out, changes);
                next.push(speccoder_core::next::NextStep {
                    command: "speccoder config commit".to_owned(),
                    why: "speccoder reads committed configuration only".to_owned(),
                });
                let mut message =
                    format!("changed {} — commit it to put it in force", keys.join(", "));
                for n in notes {
                    message.push('\n');
                    message.push_str(&n);
                }
                CommandResult::new("config", CommandStatus::Ok, &message)
                    .with_next(next)
                    .with_data(
                        r.data()
                            .cloned()
                            .unwrap_or(CommandData::Config(Box::new(read(root)))),
                    )
            }
            Err(e) => CommandResult::new(
                "config",
                CommandStatus::Error,
                &format!("{}: {e}", path.display()),
            ),
        },
    }
}

/// `speccoder config commit`: commits exactly `speccoder.toml`, nothing else
/// staged, once every parser accepts it.
#[must_use]
pub fn commit(root: &Path) -> CommandResult {
    let dir = home(root);
    let text = match std::fs::read_to_string(dir.join(FILE)) {
        Ok(t) => t,
        Err(_) => {
            return CommandResult::new(
                "config",
                CommandStatus::ConfigError,
                "there is no speccoder.toml to commit",
            )
            .with_code("CONFIG-MISSING");
        }
    };
    if let Err(e) = validate(&text) {
        return CommandResult::new("config", CommandStatus::ConfigError, &e)
            .with_code("CONFIG-INVALID");
    }
    let (committed, _) = state(&dir, true);
    if committed {
        return CommandResult::new(
            "config",
            CommandStatus::Ok,
            "speccoder.toml is committed; nothing to do",
        );
    }
    // Which sections changed, for the message.
    let g = Git::at(&dir);
    let before: toml::Table = g
        .run(&["show", &format!("HEAD:./{FILE}")])
        .ok()
        .and_then(|b| toml::from_str(&String::from_utf8_lossy(&b)).ok())
        .unwrap_or_default();
    let after: toml::Table = toml::from_str(&text).unwrap_or_default();
    let mut changed: Vec<&str> = CATALOGUE
        .iter()
        .map(|s| s.name)
        .filter(|s| before.get(*s) != after.get(*s))
        .collect();
    changed.dedup();
    let message = if changed.is_empty() {
        "speccoder: configuration".to_owned()
    } else {
        format!("speccoder: configuration ({})", changed.join(", "))
    };
    match g
        .run(&["add", "--", FILE])
        .and_then(|_| g.run(&["commit", "-q", "-m", &message, "--", FILE]))
    {
        Ok(_) => {
            let id = g
                .line(&["rev-parse", "--short", "HEAD"])
                .unwrap_or_default();
            CommandResult::new(
                "config",
                CommandStatus::Ok,
                &format!("committed speccoder.toml as {id} \"{message}\""),
            )
            .with_data(CommandData::Config(Box::new(read(root))))
        }
        Err(e) => CommandResult::new(
            "config",
            CommandStatus::GitPrecondition,
            &format!("commit failed: {}", e.0),
        ),
    }
}
