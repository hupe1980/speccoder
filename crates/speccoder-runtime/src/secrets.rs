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
    /// The value itself.
    value: String,
    /// The value and its encodings, longest first.
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

/// Masks `value` (the variable `name`'s) from now on, in this process;
/// once per name and value.
pub fn register(name: &str, value: &str) {
    if !worth_masking(value) {
        return;
    }
    let Ok(mut m) = MASKED.write() else { return };
    if m.iter().any(|x| x.name == name && x.value == value) {
        return;
    }
    m.push(Masked {
        name: name.to_owned(),
        value: value.to_owned(),
        forms: forms(value),
    });
    // Longer values first: one that contains another is masked whole.
    m.sort_by_key(|x| std::cmp::Reverse(x.value.len()));
}

/// The longest form registered, in bytes: how far a value can straddle a
/// cut.
#[must_use]
pub fn longest() -> usize {
    MASKED
        .read()
        .ok()
        .and_then(|m| m.iter().flat_map(|x| x.forms.iter().map(String::len)).max())
        .unwrap_or(0)
}

/// Whether a variable's name says its value is secret: what the commands'
/// environment drops ([`crate::verify::process::is_secret`]), and names
/// with `KEY`, `PASS`, `PWD`, `CREDENTIAL`, `AUTH`, `PRIVATE`, `TOKEN` or
/// `DSN`.
#[must_use]
pub fn secret_name(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    crate::verify::process::is_secret(&n)
        || [
            "KEY",
            "PASS",
            "PWD",
            "CREDENTIAL",
            "AUTH",
            "PRIVATE",
            "TOKEN",
            "DSN",
        ]
        .iter()
        .any(|w| n.contains(w))
}

/// The password of a URL with credentials (`scheme://user:pass@host`).
fn url_password(value: &str) -> Option<&str> {
    let (_, rest) = value.split_once("://")?;
    let (userinfo, _) = rest.split_once('@')?;
    let (_, pass) = userinfo.split_once(':')?;
    (!pass.is_empty() && !pass.contains('/')).then_some(pass)
}

/// What a copied secret file's text gives to mask, as `(NAME, value)`. A
/// dotenv file (named `.env*`): the variables whose name says secret
/// ([`secret_name`]) — each line of a multi-line value too — and any URL
/// with credentials, whole and its password. Any other file (a key, a
/// `.netrc`): each line, without PEM's `-----BEGIN …-----` markers, under
/// the file's name, and each line's last word.
#[must_use]
pub fn to_mask(file_name: &str, text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let lines = |name: &str, text: &str, out: &mut Vec<(String, String)>| {
        for l in text.lines().map(str::trim) {
            if l.starts_with("-----") && l.ends_with("-----") {
                continue;
            }
            out.push((name.to_owned(), l.to_owned()));
            if let Some(last) = l.rsplit([' ', '=', '\t']).next() {
                out.push((name.to_owned(), last.to_owned()));
            }
        }
    };
    if file_name.starts_with(".env") {
        for (k, v) in dotenv(text) {
            if let Some(pass) = url_password(&v) {
                out.push((k.clone(), pass.to_owned()));
                out.push((k.clone(), v.clone()));
            }
            if secret_name(&k) {
                if v.contains('\n') {
                    lines(&k, &v, &mut out);
                }
                out.push((k, v));
            }
        }
    } else {
        lines(file_name, text, &mut out);
    }
    out.retain(|(_, v)| worth_masking(v));
    out.dedup();
    out
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
            if out.windows(f.len()).any(|w| w == f.as_bytes()) {
                out = replace_bytes(&out, f.as_bytes(), with.as_bytes());
            }
        }
    }
    out
}

fn replace_bytes(hay: &[u8], needle: &[u8], with: &[u8]) -> Vec<u8> {
    if needle.is_empty() || !hay.windows(needle.len()).any(|w| w == needle) {
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

/// The `NAME=value` pairs of a dotenv file: `export` and quotes dropped
/// (a quoted value may span lines), comments and blank lines skipped.
#[must_use]
pub fn dotenv(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut lines = text.lines();
    while let Some(l) = lines.next() {
        let l = l.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let l = l.strip_prefix("export ").unwrap_or(l);
        let Some((k, v)) = l.split_once('=') else {
            continue;
        };
        let v = v.trim();
        let v = match v.chars().next() {
            Some(q @ ('"' | '\'')) => {
                let mut value = v[1..].to_owned();
                // Unclosed on its line: the value goes on to the closing quote.
                while !value.contains(q) {
                    let Some(next) = lines.next() else { break };
                    value.push('\n');
                    value.push_str(next);
                }
                value.split(q).next().unwrap_or_default().to_owned()
            }
            // An unquoted value ends at a comment.
            _ => v.split(" #").next().unwrap_or_default().trim().to_owned(),
        };
        let k = k.trim();
        if !k.is_empty() && !v.is_empty() {
            out.push((k.to_owned(), v));
        }
    }
    out
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
    fn only_secret_named_variables_and_credentials_in_urls_are_masked() {
        let env = "NODE_ENV=development\nDB_HOST=localhost\nDB_USER=postgres\nDB_PASSWORD=hunter2hunter2\nSTRIPE_SECRET_KEY=sk_test_abcdef\nDATABASE_URL=postgres://u:pAss-w0rd-x@h/db\n";
        let got: Vec<(String, String)> = to_mask(".env", env);
        let values: Vec<&str> = got.iter().map(|(_, v)| v.as_str()).collect();
        for kept in ["development", "localhost", "postgres"] {
            assert!(!values.contains(&kept), "{kept} masked: {got:?}");
        }
        assert_eq!(
            values,
            [
                "hunter2hunter2",
                "sk_test_abcdef",
                "pAss-w0rd-x",
                "postgres://u:pAss-w0rd-x@h/db"
            ]
        );
    }

    #[test]
    fn a_key_file_and_a_multi_line_value_are_masked_line_by_line() {
        let pem = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA\nQyNTUxOQAAACBlYmxvY2s=\n-----END OPENSSH PRIVATE KEY-----\n";
        let got = to_mask("id_ed25519", pem);
        let values: Vec<&str> = got.iter().map(|(_, v)| v.as_str()).collect();
        assert!(values.contains(&"b3BlbnNzaC1rZXktdjEAAAAA"), "{got:?}");
        assert!(values.contains(&"QyNTUxOQAAACBlYmxvY2s="), "{got:?}");
        assert!(!values.iter().any(|v| v.contains("BEGIN")), "{got:?}");
        assert_eq!(got[0].0, "id_ed25519");
        let env = format!("PRIVATE_KEY=\"{}\"\nOTHER=1\n", pem.trim_end());
        let got = to_mask(".env", &env);
        let values: Vec<&str> = got.iter().map(|(_, v)| v.as_str()).collect();
        assert!(values.contains(&"b3BlbnNzaC1rZXktdjEAAAAA"), "{got:?}");
        assert_eq!(dotenv(&env)[1], ("OTHER".to_owned(), "1".to_owned()));
        let netrc = to_mask(".netrc", "machine h login me password n3trc-secret\n");
        assert!(netrc.iter().any(|(_, v)| v == "n3trc-secret"), "{netrc:?}");
    }

    #[test]
    fn a_value_registered_twice_is_kept_once() {
        let count = || {
            MASKED
                .read()
                .unwrap()
                .iter()
                .filter(|x| x.value == "twice-registered-v4l")
                .count()
        };
        register("TWICE", "twice-registered-v4l");
        register("TWICE", "twice-registered-v4l");
        assert_eq!(count(), 1);
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
