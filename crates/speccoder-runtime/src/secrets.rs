//! Values copied into a worktree from the person's secret files (a `.env`
//! that `[worktree] include` copies): masked in everything speccoder shows
//! or hands on — command and check output, tool results, messages — as
//! `[redacted:NAME]`, each in the forms it may be printed in (as is,
//! base64, hex, URL-encoded).
//!
//! The values are held for the process, never written anywhere: masking
//! more than one worktree's values everywhere is the safe direction.

use std::path::Path;
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
/// Well-known shapes of secrets, by the name they are masked under: one
/// list for every caller (masking, the attestation's refusal, an external
/// agent's reads, the doctor's report). Most specific first.
const SHAPES: &[(&str, &str)] = &[
    (
        "PRIVATE_KEY",
        r"-----BEGIN [A-Z ]*PRIVATE KEY-----(?s:.*?)-----END [A-Z ]*PRIVATE KEY-----",
    ),
    ("AWS_ACCESS_KEY", r"\bAKIA[0-9A-Z]{16}\b"),
    (
        "GITHUB_TOKEN",
        r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}",
    ),
    ("GITHUB_TOKEN", r"\bgithub_pat_[A-Za-z0-9_]{20,}"),
    ("SLACK_TOKEN", r"\bxox[abprs]-[A-Za-z0-9-]{10,}"),
    ("ANTHROPIC_API_KEY", r"\bsk-ant-[A-Za-z0-9_-]{16,}"),
    ("OPENAI_API_KEY", r"\bsk-(?:proj-)?[A-Za-z0-9_-]{20,}"),
];

fn shapes() -> &'static [(&'static str, regex::Regex)] {
    static COMPILED: std::sync::OnceLock<Vec<(&'static str, regex::Regex)>> =
        std::sync::OnceLock::new();
    COMPILED.get_or_init(|| {
        SHAPES
            .iter()
            .filter_map(|(n, p)| regex::Regex::new(p).ok().map(|r| (*n, r)))
            .collect()
    })
}

/// The name of the first well-known secret shape `text` holds, if any.
#[must_use]
pub fn shape_in(text: &str) -> Option<&'static str> {
    shapes()
        .iter()
        .find(|(_, r)| r.is_match(text))
        .map(|(n, _)| *n)
}

/// `text` with every well-known secret shape replaced by `[redacted:NAME]`.
#[must_use]
pub fn mask_shapes(text: &str) -> String {
    let mut out = std::borrow::Cow::Borrowed(text);
    for (name, r) in shapes() {
        if r.is_match(&out) {
            out = std::borrow::Cow::Owned(
                r.replace_all(&out, format!("[redacted:{name}]").as_str())
                    .into_owned(),
            );
        }
    }
    out.into_owned()
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

/// [`mask`], and every well-known secret shape too: for what speccoder
/// writes into history or sends away — commit messages, a pull request's
/// body, an outbox, MCP replies. What a tool reads stays
/// [`mask`]ed only, so a test fixture's fake token reads as written.
#[must_use]
pub fn mask_out(text: &str) -> String {
    mask_shapes(&mask(text))
}

/// [`mask_out`] over every string in a JSON value.
pub fn mask_json_out(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::String(s) => {
            let masked = mask_out(s);
            if masked != *s {
                *s = masked;
            }
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(mask_json_out),
        serde_json::Value::Object(o) => o.values_mut().for_each(mask_json_out),
        _ => {}
    }
}

/// Whether an environment variable's name says its value is a secret:
/// narrower than [`secret_name`] (which also covers a secret file's
/// lines), so a region, a profile, a model or a base address stays as is.
#[must_use]
pub fn env_secret_name(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    let plain = [
        "_REGION", "_PROFILE", "_MODEL", "_URL", "_HOST", "_PORT", "_NAME", "_EMAIL", "_ID",
    ];
    if plain.iter().any(|p| n.ends_with(p)) && !n.ends_with("_KEY_ID") {
        return false;
    }
    [
        "_KEY",
        "_KEY_ID",
        "_TOKEN",
        "_SECRET",
        "_PASSWORD",
        "_PASSWD",
        "_PAT",
        "_CREDENTIALS",
    ]
    .iter()
    .any(|s| n.ends_with(s))
        || n.contains("PASSWORD")
        || n.contains("SECRET")
        || n == "AWS_SESSION_TOKEN"
}

/// Registers the values of this process's environment variables whose
/// names say secret ([`env_secret_name`]) — never a path.
pub fn register_env() {
    for (k, v) in std::env::vars() {
        let path = v.starts_with(['/', '~', '.']);
        if env_secret_name(&k) && !path {
            register(&k, &v);
        }
    }
}

/// Registers what a project's secret files give to mask, in any process
///: the environment ([`register_env`]), each worktree's copied
/// files (its include record), and the checkout's own ignored `.env*`.
pub fn register_project(root: &Path) {
    register_env();
    let git = crate::git::Git::at(root);
    let listed = git
        .run(&["worktree", "list", "--porcelain", "-z"])
        .map(|o| crate::git::nul_fields(&o))
        .unwrap_or_default();
    for wt in listed.iter().filter_map(|l| l.strip_prefix("worktree ")) {
        crate::include::register(Path::new(wt));
    }
    for f in crate::include::matched(root, &[".env".to_owned(), ".env.*".to_owned()]) {
        let name = Path::new(&f)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Ok(text) = std::fs::read_to_string(root.join(&f)) {
            for (k, v) in to_mask(&name, &text) {
                register(&k, &v);
            }
        }
    }
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
    fn well_known_shapes_are_masked_by_name() {
        let gh = format!("ghp_{}", "a".repeat(36));
        let ant = format!("sk-ant-api03-{}", "b".repeat(30));
        let text = format!("token {gh} and {ant} and AKIAABCDEFGHIJKLMNOP; sk-short stays");
        assert_eq!(
            mask(&text),
            text,
            "a tool's read keeps a fixture's fake token"
        );
        let out = mask_out(&text);
        assert!(out.contains("[redacted:GITHUB_TOKEN]"), "{out}");
        assert!(out.contains("[redacted:ANTHROPIC_API_KEY]"), "{out}");
        assert!(out.contains("[redacted:AWS_ACCESS_KEY]"), "{out}");
        assert!(out.contains("sk-short stays"), "{out}");
        assert_eq!(shape_in(&gh), Some("GITHUB_TOKEN"));
        assert_eq!(shape_in("nothing here"), None);
        let pem =
            "a\n-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----\nb";
        assert_eq!(mask_out(pem), "a\n[redacted:PRIVATE_KEY]\nb");
        let open = "x -----BEGIN PRIVATE KEY----- y and more";
        assert_eq!(mask_out(open), open, "no END marker: nothing to cut");
        for (n, secret) in [
            ("OPENAI_API_KEY", true),
            ("AWS_SECRET_ACCESS_KEY", true),
            ("AWS_ACCESS_KEY_ID", true),
            ("GH_TOKEN", true),
            ("DB_PASSWORD", true),
            ("AWS_REGION", false),
            ("AWS_PROFILE", false),
            ("ANTHROPIC_MODEL", false),
            ("OPENAI_BASE_URL", false),
            ("GIT_AUTHOR_NAME", false),
            ("SSH_AUTH_SOCK", false),
            ("PWD", false),
        ] {
            assert_eq!(env_secret_name(n), secret, "{n}");
        }
    }

    #[test]
    fn a_fresh_process_masks_a_project_s_dotenv_and_secret_named_variables() {
        let t = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let root = t.path();
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .status()
                    .is_ok_and(|s| s.success())
            );
        };
        git(&["init", "-q"]);
        std::fs::write(root.join(".gitignore"), ".env\n").unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(
            root.join(".env"),
            "API_TOKEN=canary-7f3a9c2e51\nPORT=5432\n",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        register_project(root);
        assert_eq!(
            mask("got canary-7f3a9c2e51 back"),
            "got [redacted:API_TOKEN] back"
        );
        assert_eq!(mask("port 5432"), "port 5432");
    }

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
