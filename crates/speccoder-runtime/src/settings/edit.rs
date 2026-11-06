//! Edits of `speccoder.toml` that keep its comments: a dotted key's place,
//! its value checked against its kind, the change checked by every parser.

use serde_json::{Value, json};

use speccoder_core::ConfigChange;

use super::catalogue::{CATALOGUE, Key, Kind};
use super::{json_of, lookup, validate_but_model};

/// Where a dotted key lives: the table path and the leaf, with its catalogue
/// entry.
pub(super) struct Place {
    pub(super) tables: Vec<String>,
    pub(super) leaf: String,
    pub(super) key: &'static Key,
}

pub(super) fn place(dotted: &str) -> Result<Place, String> {
    let parts: Vec<&str> = dotted.split('.').collect();
    let Some(section) = parts
        .first()
        .and_then(|s| CATALOGUE.iter().find(|c| c.name == *s))
    else {
        if parts.first() == Some(&"ui") {
            return Err("[ui] is a person's setting, not the project's: it belongs in ~/.config/speccoder/config.toml".to_owned());
        }
        return Err(format!(
            "unknown section in `{dotted}` (known: {})",
            CATALOGUE
                .iter()
                .map(|c| c.name)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };
    let (tables, rest): (Vec<String>, &[&str]) = if section.named {
        let Some(name) = parts.get(1) else {
            return Err(format!(
                "`{dotted}`: name the table, e.g. {}.<name>.<key>",
                section.name
            ));
        };
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return Err(format!(
                "`{name}`: a table name is lowercase letters, digits, - and _"
            ));
        }
        (
            vec![section.name.to_owned(), (*name).to_owned()],
            &parts[2..],
        )
    } else {
        (vec![section.name.to_owned()], &parts[1..])
    };
    let wanted = rest.join(".");
    let key = section
        .keys
        .iter()
        .find(|k| k.key == wanted)
        .ok_or_else(|| {
            format!(
                "unknown key `{dotted}` (in [{}]: {})",
                section.name,
                section
                    .keys
                    .iter()
                    .map(|k| k.key)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    let mut tables = tables;
    let mut leaf = wanted.clone();
    if let Some((sub, last)) = wanted.rsplit_once('.') {
        tables.extend(sub.split('.').map(str::to_owned));
        leaf = last.to_owned();
    }
    Ok(Place { tables, leaf, key })
}

/// Checks `value` against `kind` before any parser sees it, for a message
/// that names the key.
pub(super) fn check(dotted: &str, kind: Kind, value: &Value) -> Result<(), String> {
    let strings = |v: &Value| v.as_array().is_some_and(|a| a.iter().all(Value::is_string));
    let ok = match kind {
        Kind::Bool => value.is_boolean(),
        Kind::Int(a, b) => value.as_i64().is_some_and(|n| (a..=b).contains(&n)),
        Kind::Num(a, b) => value.as_f64().is_some_and(|n| n >= a && n <= b),
        Kind::Str => value.as_str().is_some_and(|s| !s.is_empty()),
        Kind::Choice(c) => value.as_str().is_some_and(|s| c.contains(&s)),
        Kind::Duration => value.as_str().is_some_and(|s| {
            // The unit is the last character, which may be more than a byte.
            let (n, u) = s.split_at(s.char_indices().last().map_or(0, |(i, _)| i));
            n.parse::<u64>().is_ok_and(|n| n > 0) && ["m", "h", "d"].contains(&u)
        }),
        Kind::Argv => strings(value) && value.as_array().is_some_and(|a| !a.is_empty()),
        Kind::ArgvList => value.as_array().is_some_and(|a| {
            a.iter()
                .all(|x| strings(x) && x.as_array().is_some_and(|i| !i.is_empty()))
        }),
        Kind::StrList => strings(value),
    };
    if ok {
        return Ok(());
    }
    let want = match kind {
        Kind::Bool => "true or false".to_owned(),
        Kind::Int(a, b) => format!("a whole number from {a} to {b}"),
        Kind::Num(a, b) => format!("a number from {a} to {b}"),
        Kind::Str => "text".to_owned(),
        Kind::Choice(c) => format!("one of {}", c.join(", ")),
        Kind::Duration => "a duration like 30m, 12h or 7d".to_owned(),
        Kind::Argv => "a program and its arguments, e.g. [\"cargo\", \"fmt\"]".to_owned(),
        Kind::ArgvList => {
            "a list of programs with arguments, e.g. [[\"cargo\", \"check\"]]".to_owned()
        }
        Kind::StrList => "a list of strings".to_owned(),
    };
    Err(format!("`{dotted}` = {value}: expected {want}"))
}

pub(super) fn toml_value(v: &Value) -> Option<toml_edit::Value> {
    Some(match v {
        Value::Bool(b) => toml_edit::Value::from(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => toml_edit::Value::from(i),
            None => toml_edit::Value::from(n.as_f64()?),
        },
        Value::String(s) => toml_edit::Value::from(s.as_str()),
        Value::Array(a) => {
            let mut arr = toml_edit::Array::new();
            for x in a {
                arr.push(toml_value(x)?);
            }
            toml_edit::Value::Array(arr)
        }
        Value::Null | Value::Object(_) => return None,
    })
}

/// The table at `path` — a `[table]` or an inline `{ … }` — made when
/// missing and `create`; `None` when missing and not. The containers of
/// named tables (`[checks]`, `[mcp]`) are implicit: no header of their own.
pub(super) fn table_mut<'a>(
    doc: &'a mut toml_edit::DocumentMut,
    path: &[String],
    create: bool,
) -> Result<Option<&'a mut dyn toml_edit::TableLike>, String> {
    let mut t: &mut dyn toml_edit::TableLike = doc.as_table_mut();
    let mut inline = false;
    for (i, p) in path.iter().enumerate() {
        if t.get(p).is_none() {
            if !create {
                return Ok(None);
            }
            let fresh = if inline {
                // Inside an inline table, what is made is inline too.
                toml_edit::Item::Value(toml_edit::Value::InlineTable(toml_edit::InlineTable::new()))
            } else {
                let mut table = toml_edit::Table::new();
                let container = i == 0 && CATALOGUE.iter().any(|c| c.named && c.name == p);
                table.set_implicit(container || i + 1 < path.len());
                toml_edit::Item::Table(table)
            };
            t.insert(p, fresh);
        }
        let Some(item) = t.get_mut(p) else {
            return Err(format!("`{}` could not be made", path[..=i].join(".")));
        };
        inline = inline || item.is_inline_table();
        t = item
            .as_table_like_mut()
            .ok_or_else(|| format!("`{}` is not a table", path[..=i].join(".")))?;
    }
    Ok(Some(t))
}

/// Removes empty tables (or inline tables) left behind by an unset,
/// innermost first.
pub(super) fn prune(doc: &mut toml_edit::DocumentMut, path: &[String]) {
    for depth in (1..=path.len()).rev() {
        let (parent, last) = (&path[..depth - 1], &path[depth - 1]);
        let Ok(Some(t)) = table_mut(doc, parent, false) else {
            return;
        };
        let empty = t
            .get(last)
            .and_then(toml_edit::Item::as_table_like)
            .is_some_and(toml_edit::TableLike::is_empty);
        if empty {
            t.remove(last);
        }
    }
}

/// The new text with `changes` applied, or why not.
///
/// # Errors
///
/// An unknown key, a value of the wrong kind, or a result a parser refuses.
pub fn edit(text: &str, changes: &[ConfigChange]) -> Result<String, String> {
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| e.to_string())?;
    for c in changes {
        let at = place(&c.key)?;
        match (&c.value, &c.append) {
            (Some(v), None) => {
                check(&c.key, at.key.kind, v)?;
                let value = toml_value(v)
                    .ok_or_else(|| format!("`{}`: a value TOML cannot hold", c.key))?;
                let t = table_mut(&mut doc, &at.tables, true)?
                    .ok_or_else(|| format!("`{}` could not be made", c.key))?;
                match t.get_mut(&at.leaf).and_then(toml_edit::Item::as_value_mut) {
                    // Keep the key's own comment and spacing.
                    Some(old) => {
                        let decor = old.decor().clone();
                        *old = value;
                        *old.decor_mut() = decor;
                    }
                    None => {
                        t.insert(&at.leaf, toml_edit::value(value));
                    }
                }
            }
            (None, Some(item)) => {
                if !matches!(at.key.kind, Kind::ArgvList | Kind::StrList) {
                    return Err(format!("`{}` is not a list", c.key));
                }
                // The list as it is now, read back through the parser.
                let now: toml::Table = toml::from_str(&doc.to_string()).unwrap_or_default();
                let mut path: Vec<&str> = at.tables.iter().map(String::as_str).collect();
                path.push(&at.leaf);
                let mut list: Vec<Value> = lookup(&now, &path)
                    .map(json_of)
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default();
                if !list.contains(item) {
                    list.push(item.clone());
                }
                let all = Value::Array(list);
                check(&c.key, at.key.kind, &all)?;
                let value = toml_value(&all)
                    .ok_or_else(|| format!("`{}`: a value TOML cannot hold", c.key))?;
                table_mut(&mut doc, &at.tables, true)?
                    .ok_or_else(|| format!("`{}` could not be made", c.key))?
                    .insert(&at.leaf, toml_edit::value(value));
            }
            (None, None) => {
                // Missing is already unset; anything else in the way is an error.
                if let Some(t) = table_mut(&mut doc, &at.tables, false)? {
                    t.remove(&at.leaf);
                }
                prune(&mut doc, &at.tables);
            }
            (Some(_), Some(_)) => {
                return Err(format!(
                    "`{}`: set a value or append to it, not both",
                    c.key
                ));
            }
        }
    }
    let out = doc.to_string();
    match crate::substrate::model_config::parse(&out) {
        // A step on the way to Bedrock: its provider before its region,
        // which `config set model.region` adds next (until then every
        // command, and `config commit`, refuses the file and says so).
        Err(e) if e.starts_with(crate::substrate::model_config::NEEDS_REGION) => {}
        other => {
            other?;
        }
    }
    validate_but_model(&out)?;
    Ok(out)
}

/// A value for `key` from the words of `config set KEY WORDS…`: JSON when it
/// parses as the key's kind, else words read by the kind (`cargo fmt` for an
/// argv, `40` for a number, `on`/`off` for a switch).
///
/// # Errors
///
/// An unknown key, or words that are no value of its kind.
pub fn value_of(key: &str, words: &[String]) -> Result<Value, String> {
    let at = place(key)?;
    let joined = words.join(" ");
    if let Ok(v) = serde_json::from_str::<Value>(&joined)
        && check(key, at.key.kind, &v).is_ok()
    {
        return Ok(v);
    }
    let v = match at.key.kind {
        Kind::Bool => match joined.as_str() {
            "true" | "on" | "yes" => json!(true),
            "false" | "off" | "no" => json!(false),
            _ => json!(joined),
        },
        Kind::Int(..) => joined
            .parse::<i64>()
            .map_or_else(|_| json!(joined), |n| json!(n)),
        Kind::Num(..) => joined
            .parse::<f64>()
            .map_or_else(|_| json!(joined), |n| json!(n)),
        Kind::Argv | Kind::StrList => json!(words),
        Kind::ArgvList => json!([words]),
        Kind::Str | Kind::Choice(_) | Kind::Duration => json!(joined),
    };
    check(key, at.key.kind, &v).map(|()| v)
}
