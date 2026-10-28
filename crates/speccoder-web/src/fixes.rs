//! The person's own commands from the page: only what this host issued.
//!
//! A fix speccoder printed that is the person's own command (`npm install`)
//! reaches the page as exact argvs ([`How::Command`]). As the host sends
//! it, it signs those argvs for its project with a key the page never
//! sees; `POST /api/fix/run` runs argvs only with their signature. So the
//! page can run what speccoder printed, and nothing it composed, edited or
//! copied from a model's words. The signature is an HMAC-SHA-256 over the
//! project folder and the argvs.
//!
//! [`How::Command`]: speccoder_core::step::How::Command

use std::path::Path;

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Signs and checks the argvs this host issued.
#[derive(Debug)]
pub struct Issuer {
    key: [u8; 32],
}

/// The keys under which a result holds steps: its fixes, its next steps,
/// its payload's checks and items, an action's step, a job's result.
const HOLDERS: &[&str] = &[
    "fix_steps",
    "next",
    "data",
    "checks",
    "items",
    "actions",
    "step",
    "result",
];

impl Issuer {
    /// A fresh key; `None` without an OS random source.
    #[must_use]
    pub fn new() -> Option<Self> {
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).ok()?;
        Some(Self { key })
    }

    fn mac(&self, root: &Path, argvs: &[Vec<String>]) -> String {
        const BLOCK: usize = 64;
        let mut k = [0u8; BLOCK];
        k[..self.key.len()].copy_from_slice(&self.key);
        let message = serde_json::to_vec(&(root.display().to_string(), argvs)).unwrap_or_default();
        let pad = |b: u8| k.iter().map(|x| x ^ b).collect::<Vec<u8>>();
        let inner = Sha256::new()
            .chain_update(pad(0x36))
            .chain_update(&message)
            .finalize();
        let outer = Sha256::new()
            .chain_update(pad(0x5c))
            .chain_update(inner)
            .finalize();
        outer.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Whether `token` is this host's signature of `argvs` in `root`.
    #[must_use]
    pub fn issued(&self, root: &Path, argvs: &[Vec<String>], token: &str) -> bool {
        !argvs.is_empty()
            && argvs.iter().all(|a| !a.is_empty())
            && crate::security::constant_time_eq(self.mac(root, argvs).as_bytes(), token.as_bytes())
    }

    /// Signs every command step of `v`, a result this host sends for the
    /// project at `root`. Only where results hold steps ([`HOLDERS`]):
    /// anything else (a file's content) is never signed.
    pub fn sign(&self, root: &Path, v: &mut Value) {
        match v {
            Value::Array(items) => {
                for item in items {
                    self.sign(root, item);
                }
            }
            Value::Object(map) => {
                if map.get("how").and_then(Value::as_str) == Some("command") {
                    let argvs: Option<Vec<Vec<String>>> = map
                        .get("argvs")
                        .and_then(|a| serde_json::from_value(a.clone()).ok());
                    if let Some(argvs) = argvs.filter(|a| !a.is_empty()) {
                        map.insert("token".to_owned(), Value::String(self.mac(root, &argvs)));
                    }
                }
                for key in HOLDERS {
                    if let Some(child) = map.get_mut(*key) {
                        self.sign(root, child);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn only_what_it_signed_runs() {
        let issuer = Issuer::new().unwrap_or_else(|| panic!("no random source"));
        let root = Path::new("/p");
        let mut v = json!({
            "fix_steps": [
                { "command": "npm install", "how": "command", "argvs": [["npm", "install"]], "interactive": false },
                { "command": "export X=…", "how": "shell", "why": "", "setting": null }
            ],
            "data": { "checks": [{ "fix_steps": [{ "how": "command", "argvs": [["uv", "sync"]] }] }],
                      "content": { "how": "command", "argvs": [["rm", "-rf", "/"]] } }
        });
        issuer.sign(root, &mut v);
        let token = v["fix_steps"][0]["token"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert_eq!(token.len(), 64);
        assert!(issuer.issued(root, &[s(&["npm", "install"])], &token));
        assert!(v["data"]["checks"][0]["fix_steps"][0]["token"].is_string());
        // A shell line is never a command; what no result holds is never signed.
        assert!(v["fix_steps"][1].get("token").is_none());
        assert!(v["data"]["content"].get("token").is_none());
        // Changed argvs, another project, another host's key, no token.
        assert!(!issuer.issued(root, &[s(&["npm", "install", "evil"])], &token));
        assert!(!issuer.issued(Path::new("/q"), &[s(&["npm", "install"])], &token));
        let other = Issuer::new().unwrap_or_else(|| panic!("no random source"));
        assert!(!other.issued(root, &[s(&["npm", "install"])], &token));
        assert!(!issuer.issued(root, &[s(&["npm", "install"])], ""));
        assert!(!issuer.issued(root, &[], &issuer.mac(root, &[])));
    }
}
