//! The web app's types, generated from the published JSON Schemas: one
//! TypeScript declaration per `$defs` entry and per schema root, so the
//! page cannot drift from what the host sends. Only the keywords the
//! schemas use are understood; any other is an error, never `unknown`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::{Map, Value};

/// Keywords that say nothing about the shape of a value in TypeScript.
const IGNORED: &[&str] = &[
    "description",
    "format",
    "minimum",
    "maximum",
    "minItems",
    "maxItems",
    "default",
    "title",
    "$id",
    "$schema",
    "$defs",
];

/// Keywords rendered.
const SHAPES: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "prefixItems",
    "const",
    "oneOf",
    "anyOf",
    "$ref",
];

/// The file's header.
pub const HEADER: &str = "// Generated from site/static/schemas/v2 by `SPECCODER_BLESS=1 cargo test -p speccoder ui_types`. Do not edit.\n";

/// The declarations of `schemas` (each `(schema name, root type name,
/// schema)`), in name order.
///
/// # Errors
///
/// A keyword this renderer does not know, or one name declared
/// differently by two schemas.
pub fn render(schemas: &[(&str, &str, Value)]) -> Result<String, String> {
    // name → (declaration, the schema that first declared it)
    let mut declared: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut declare = |name: &str, schema: &Value, from: &str| -> Result<(), String> {
        let text = declaration(name, schema).map_err(|e| format!("{from}: {name}: {e}"))?;
        match declared.get(name) {
            Some((t, first)) if *t != text => Err(format!(
                "`{name}` is declared differently by {first} and {from}"
            )),
            Some(_) => Ok(()),
            None => {
                declared.insert(name.to_owned(), (text, from.to_owned()));
                Ok(())
            }
        }
    };
    for (file, root, schema) in schemas {
        if let Some(defs) = schema.get("$defs").and_then(Value::as_object) {
            for (name, def) in defs {
                declare(name, def, file)?;
            }
        }
        declare(root, schema, file)?;
    }
    let mut out = String::from(HEADER);
    for (text, _) in declared.values() {
        out.push('\n');
        out.push_str(text);
    }
    Ok(out)
}

/// `export interface Name { … }` for an object, `export type Name = …`
/// otherwise; its description as a doc comment.
fn declaration(name: &str, schema: &Value) -> Result<String, String> {
    let mut out = doc(schema, "");
    let plain_object = schema.get("type").and_then(Value::as_str) == Some("object")
        && schema.get("properties").is_some()
        && schema.get("oneOf").is_none()
        && schema.get("anyOf").is_none();
    if plain_object {
        let _ = writeln!(out, "export interface {name} {}", object(schema, "")?);
    } else {
        let e = expr(schema, "")?;
        let sep = if e.starts_with('\n') { "" } else { " " };
        let _ = writeln!(out, "export type {name} ={sep}{e};");
    }
    Ok(out)
}

/// A `/** … */` comment of the schema's description, at `indent`.
fn doc(schema: &Value, indent: &str) -> String {
    let Some(text) = schema.get("description").and_then(Value::as_str) else {
        return String::new();
    };
    let text = text.replace("*/", "*\\/");
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() == 1 {
        return format!("{indent}/** {} */\n", lines[0]);
    }
    let mut out = format!("{indent}/**\n");
    for l in lines {
        let _ = writeln!(out, "{indent} *{}{l}", if l.is_empty() { "" } else { " " });
    }
    let _ = writeln!(out, "{indent} */");
    out
}

/// The TypeScript type of `schema`, written at `indent`.
fn expr(schema: &Value, indent: &str) -> Result<String, String> {
    let Some(o) = schema.as_object() else {
        // `true`: any value.
        return match schema {
            Value::Bool(true) => Ok("unknown".to_owned()),
            other => Err(format!("a schema that is {other}")),
        };
    };
    if let Some(k) = o
        .keys()
        .find(|k| !IGNORED.contains(&k.as_str()) && !SHAPES.contains(&k.as_str()))
    {
        return Err(format!("the keyword `{k}` is not one the UI types render"));
    }
    if let Some(r) = o.get("$ref").and_then(Value::as_str) {
        return r
            .strip_prefix("#/$defs/")
            .map(str::to_owned)
            .ok_or_else(|| format!("a reference outside $defs: {r}"));
    }
    if let Some(c) = o.get("const") {
        return Ok(c.to_string());
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(members) = o.get(key).and_then(Value::as_array) {
            let union = union(members, indent)?;
            // Properties beside the members: every member has them too.
            return if o.contains_key("properties") {
                Ok(format!("{} & ({union})", object(schema, indent)?))
            } else {
                Ok(union)
            };
        }
    }
    match o.get("type") {
        None => Ok("unknown".to_owned()),
        Some(Value::String(t)) => typed(t, o, indent),
        Some(Value::Array(ts)) => {
            let parts: Result<Vec<String>, String> = ts
                .iter()
                .map(|t| {
                    t.as_str()
                        .ok_or_else(|| format!("a type that is {t}"))
                        .and_then(|t| typed(t, o, indent))
                })
                .collect();
            Ok(parts?.join(" | "))
        }
        Some(other) => Err(format!("a type that is {other}")),
    }
}

fn union(members: &[Value], indent: &str) -> Result<String, String> {
    let inner = format!("{indent}  ");
    let parts: Result<Vec<String>, String> = members.iter().map(|m| expr(m, &inner)).collect();
    let parts = parts?;
    if parts.iter().any(|p| p.contains('\n')) {
        // One member per line, each with its description.
        let mut out = String::new();
        for (m, p) in members.iter().zip(&parts) {
            let _ = write!(out, "\n{}{inner}| {p}", doc(m, &inner));
        }
        return Ok(out);
    }
    let flat: Result<Vec<String>, String> = members.iter().map(|m| expr(m, indent)).collect();
    Ok(flat?.join(" | "))
}

fn typed(t: &str, o: &Map<String, Value>, indent: &str) -> Result<String, String> {
    Ok(match t {
        "string" => "string".to_owned(),
        "integer" | "number" => "number".to_owned(),
        "boolean" => "boolean".to_owned(),
        "null" => "null".to_owned(),
        "array" => match (o.get("prefixItems"), o.get("items")) {
            (Some(Value::Array(items)), _) => {
                let parts: Result<Vec<String>, String> =
                    items.iter().map(|i| expr(i, indent)).collect();
                format!("[{}]", parts?.join(", "))
            }
            (None, Some(items)) => {
                let item = expr(items, indent)?;
                if item.contains(' ') && !item.starts_with('{') {
                    format!("({item})[]")
                } else {
                    format!("{item}[]")
                }
            }
            _ => "unknown[]".to_owned(),
        },
        "object" => match (o.get("properties"), o.get("additionalProperties")) {
            (Some(_), _) => object(&Value::Object(o.clone()), indent)?,
            (None, Some(v)) => format!("Record<string, {}>", expr(v, indent)?),
            (None, None) => "Record<string, unknown>".to_owned(),
        },
        other => return Err(format!("the type `{other}`")),
    })
}

/// `{ key: T; other?: U }`, one property per line with its description.
fn object(schema: &Value, indent: &str) -> Result<String, String> {
    let empty = Map::new();
    let props = schema
        .get("properties")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if props.is_empty() {
        return Ok("{}".to_owned());
    }
    let inner = format!("{indent}  ");
    let mut out = String::from("{\n");
    for (key, prop) in props {
        let name = if key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
            && !key.starts_with(|c: char| c.is_ascii_digit())
        {
            key.clone()
        } else {
            Value::String(key.clone()).to_string()
        };
        let optional = if required.contains(&key.as_str()) {
            ""
        } else {
            "?"
        };
        out.push_str(&doc(prop, &inner));
        let _ = writeln!(out, "{inner}{name}{optional}: {};", expr(prop, &inner)?);
    }
    out.push_str(indent);
    out.push('}');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn published(name: &str) -> Value {
        crate::schema::get(name).unwrap_or_else(|| panic!("no schema {name}"))
    }

    #[test]
    fn a_tagged_enum_is_a_union_on_its_tag() {
        let ts = render(&[("actions", "ActionsReport", published("actions"))])
            .unwrap_or_else(|e| panic!("{e}"));
        let runnable = ts
            .split("export type Runnable =")
            .nth(1)
            .and_then(|t| t.split("\nexport ").next())
            .unwrap_or_else(|| panic!("{ts}"));
        // One member per variant, taken from the enum, not counted here.
        let kinds: std::collections::BTreeSet<String> =
            speccoder_core::actions::Runnable::examples()
                .iter()
                .map(|r| serde_json::to_value(r).unwrap_or_default()["kind"].to_string())
                .collect();
        assert_eq!(runnable.matches("  | {").count(), kinds.len(), "{runnable}");
        for k in kinds {
            assert!(runnable.contains(&format!("kind: {k};")), "{k}: {runnable}");
        }
        assert!(
            ts.contains("export type StepState = \"done\" | \"now\" | \"todo\" | \"skipped\";"),
            "{ts}"
        );
        // Nullable and optional as the schema says.
        assert!(ts.contains("  blocked: string | null;"), "{ts}");
        assert!(ts.contains("  role: SkillRole | null;"), "{ts}");
        assert!(ts.contains("export interface ActionsReport {"), "{ts}");
    }

    #[test]
    fn what_it_does_not_know_is_an_error_and_a_clash_is_named() {
        let unknown = json!({ "type": "string", "pattern": "^T" });
        assert!(render(&[("x", "X", unknown)]).is_err_and(|e| e.contains("`pattern`")),);
        let a = json!({ "$defs": { "Same": { "type": "string" } }, "type": "null" });
        let b = json!({ "$defs": { "Same": { "type": "integer" } }, "type": "null" });
        let clash = render(&[("a", "A", a), ("b", "B", b)]);
        assert!(clash.is_err_and(|e| e.contains("`Same` is declared differently by a and b")),);
        let tuple = json!({ "type": "array", "prefixItems": [{ "type": "string" }, { "type": "integer" }] });
        let optional = json!({ "type": "object", "properties": { "in-place": { "type": "boolean" } }, "required": [] });
        let ts = render(&[("t", "T", tuple), ("o", "O", optional)]).unwrap_or_default();
        assert!(ts.contains("export type T = [string, number];"), "{ts}");
        assert!(ts.contains("  \"in-place\"?: boolean;"), "{ts}");
    }
}
