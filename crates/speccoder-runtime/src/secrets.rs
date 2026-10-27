//! Values copied into a worktree from the person's secret files (a `.env`
//! that `[worktree] include` copies): masked in everything speccoder shows
//! or hands on — command and check output, tool results, messages — as
//! `[redacted:NAME]`, each in the forms it may be printed in (as is,
//! base64, hex, URL-encoded).
//!
//! The values are held for the process, never written anywhere: masking
//! more than one worktree's values everywhere is the safe direction.

use std::sync::RwLock;

use base64::Engine as _;

/// The shortest value masked: shorter ones are flags and ports.
pub const MIN_LEN: usize = 8;

/// One value and the forms it is masked in.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Masked {
    /// The variable it is the value of.
    name: String,
    /// The value itself first, then its encodings (longest first).
    forms: Vec<String>,
}

static MASKED: RwLock<Vec<Masked>> = RwLock::new(Vec::new());

/// Whether `value` is worth masking: at least [`MIN_LEN`] long and not a
/// plain number.
#[must_use]
pub fn worth_masking(value: &str) -> bool {
    value.len() >= MIN_LEN && !value.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// The forms `value` may be printed in.
fn forms(value: &str) -> Vec<String> {
    let b = value.as_bytes();
    let mut out = vec![value.to_owned()];
    let engines = [
        base64::engine::general_purpose::STANDARD,
        base64::engine::general_purpose::STANDARD_NO_PAD,
        base64::engine::general_purpose::URL_SAFE,
        base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ];
    out.extend(engines.iter().map(|e| e.encode(b)));
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    out.push(hex.to_uppercase());
    out.push(hex);
    out.push(url_encoded(value));
    out.sort_by_key(|f| std::cmp::Reverse(f.len()));
    out.dedup();
    out
}

/// `value` percent-encoded (everything but RFC 3986's unreserved characters).
fn url_encoded(value: &str) -> String {
    value
        .bytes()
        .map(|c| {
            if c.is_ascii_alphanumeric() || b"-._~".contains(&c) {
                (c as char).to_string()
            } else {
                format!("%{c:02X}")
            }
        })
        .collect()
}

/// Masks `value` (the variable `name`'s) from now on, in this process.
pub fn register(name: &str, value: &str) {
    if !worth_masking(value) {
        return;
    }
    let Ok(mut m) = MASKED.write() else { return };
    if m.iter()
        .any(|x| x.forms.first().is_some_and(|f| f == value))
    {
        return;
    }
    m.push(Masked {
        name: name.to_owned(),
        forms: forms(value),
    });
    // Longer values first: one that contains another is masked whole.
    m.sort_by_key(|x| std::cmp::Reverse(x.forms.first().map_or(0, String::len)));
}

/// `text` with every registered value, in each of its forms, replaced by
/// `[redacted:NAME]`.
#[must_use]
pub fn mask(text: &str) -> String {
    let Ok(m) = MASKED.read() else {
        return text.to_owned();
    };
    let mut out = text.to_owned();
    for x in m.iter() {
        for f in &x.forms {
            if out.contains(f.as_str()) {
                out = out.replace(f.as_str(), &format!("[redacted:{}]", x.name));
            }
        }
    }
    out
}

/// [`mask`] over bytes that need not be UTF-8: each form replaced where it
/// occurs, every other byte kept.
#[must_use]
pub fn mask_bytes(bytes: &[u8]) -> Vec<u8> {
    let Ok(m) = MASKED.read() else {
        return bytes.to_vec();
    };
    let mut out = bytes.to_vec();
    for x in m.iter() {
        let with = format!("[redacted:{}]", x.name);
        for f in &x.forms {
            out = replace_bytes(&out, f.as_bytes(), with.as_bytes());
        }
    }
    out
}

fn replace_bytes(hay: &[u8], needle: &[u8], with: &[u8]) -> Vec<u8> {
    if needle.is_empty() || hay.len() < needle.len() {
        return hay.to_vec();
    }
    let mut out = Vec::with_capacity(hay.len());
    let mut i = 0;
    while i < hay.len() {
        if hay[i..].starts_with(needle) {
            out.extend_from_slice(with);
            i += needle.len();
        } else {
            out.push(hay[i]);
            i += 1;
        }
    }
    out
}

/// [`mask`] over every string in a JSON value.
pub fn mask_json(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::String(s) => {
            let masked = mask(s);
            if masked != *s {
                *s = masked;
            }
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(mask_json),
        serde_json::Value::Object(o) => o.values_mut().for_each(mask_json),
        _ => {}
    }
}

/// The variable whose value (as is) `text` holds, if any.
#[must_use]
pub fn found_in(text: &str) -> Option<String> {
    let m = MASKED.read().ok()?;
    m.iter()
        .find(|x| x.forms.iter().any(|f| text.contains(f.as_str())))
        .map(|x| x.name.clone())
}

/// The `NAME=value` pairs of a dotenv file: `export` and quotes dropped,
/// comments and blank lines skipped.
#[must_use]
pub fn dotenv(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() || l.starts_with('#') {
                return None;
            }
            let l = l.strip_prefix("export ").unwrap_or(l);
            let (k, v) = l.split_once('=')?;
            let v = v.trim();
            let v = match v.chars().next() {
                Some(q @ ('"' | '\'')) => v[1..].split(q).next().unwrap_or_default(),
                // An unquoted value ends at a comment.
                _ => v.split(" #").next().unwrap_or_default().trim(),
            };
            Some((k.trim().to_owned(), v.to_owned()))
        })
        .filter(|(k, v)| !k.is_empty() && !v.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dotenv_file_gives_its_pairs() {
        let pairs = dotenv(
            "# db\nexport DB_PASSWORD=\"s3cr3t pass\"\nTOKEN='abc' # x\nPORT=5432 # local\n\nEMPTY=\nbad line\n",
        );
        assert_eq!(
            pairs,
            [
                ("DB_PASSWORD".to_owned(), "s3cr3t pass".to_owned()),
                ("TOKEN".to_owned(), "abc".to_owned()),
                ("PORT".to_owned(), "5432".to_owned()),
            ]
        );
        assert!(worth_masking("s3cr3t pass"));
        assert!(!worth_masking("abc"));
        assert!(!worth_masking("12345678"));
    }

    #[test]
    fn a_value_is_masked_in_each_form_it_is_printed_in() {
        let value = "masking-test-Zq9 value/+";
        register("MASK_TEST", value);
        let b64 = base64::engine::general_purpose::STANDARD.encode(value);
        let hex: String = value.bytes().map(|x| format!("{x:02x}")).collect();
        let text = format!("a {value} b {b64} c {hex} d {}", url_encoded(value));
        assert_eq!(
            mask(&text),
            "a [redacted:MASK_TEST] b [redacted:MASK_TEST] c [redacted:MASK_TEST] d [redacted:MASK_TEST]"
        );
        assert_eq!(
            mask_bytes(format!("\u{0}{value}\u{ff}").as_bytes()),
            b"\0[redacted:MASK_TEST]\xc3\xbf".to_vec()
        );
        let mut v = serde_json::json!({"out": [format!("x{value}")]});
        mask_json(&mut v);
        assert_eq!(v["out"][0], "x[redacted:MASK_TEST]");
        assert_eq!(found_in(&format!("+ {b64}")).as_deref(), Some("MASK_TEST"));
        assert_eq!(found_in("nothing here"), None);
    }
}
