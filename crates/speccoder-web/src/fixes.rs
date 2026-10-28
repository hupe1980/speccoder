//! The person's own commands from the page: only what this host issued.
//!
//! A fix that is the person's own command reaches the page as a typed
//! step speccoder built ([`How::Command`]): exact argvs, built by their
//! source or one of speccoder's constant lines. While the host builds and
//! serialises a result, every such step is signed for its project —
//! HMAC-SHA-256 over the project folder, the argvs and whether they read
//! the keyboard, with a key the page never sees ([`Issuer::signing`]).
//! JSON from elsewhere is never signed. `POST /api/fix/run` runs argvs
//! only with their signature: what speccoder printed, and nothing the
//! page composed, edited or took from a model's words.
//!
//! [`How::Command`]: speccoder_core::step::How::Command

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};

/// Signs and checks the argvs this host issued.
pub struct Issuer {
    key: [u8; 32],
}

/// The key stays out of every log and panic message.
impl std::fmt::Debug for Issuer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Issuer { key: [redacted] }")
    }
}

impl Issuer {
    /// A fresh key; `None` without an OS random source.
    #[must_use]
    pub fn new() -> Option<Self> {
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).ok()?;
        Some(Self { key })
    }

    fn mac(&self, root: &Path, argvs: &[Vec<String>], interactive: bool) -> String {
        const BLOCK: usize = 64;
        let mut k = [0u8; BLOCK];
        k[..self.key.len()].copy_from_slice(&self.key);
        let message = serde_json::to_vec(&(root.display().to_string(), argvs, interactive))
            .unwrap_or_default();
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

    /// Whether `token` is this host's signature of `argvs` in `root`, as a
    /// step that does (`interactive`) or does not read the keyboard.
    #[must_use]
    pub fn issued(
        &self,
        root: &Path,
        argvs: &[Vec<String>],
        interactive: bool,
        token: &str,
    ) -> bool {
        !argvs.is_empty()
            && argvs.iter().all(|a| !a.is_empty())
            && crate::security::constant_time_eq(
                self.mac(root, argvs, interactive).as_bytes(),
                token.as_bytes(),
            )
    }

    /// Runs `f` with every command step speccoder builds meanwhile on this
    /// thread signed for the project at `root`.
    pub fn signing<T>(self: &Arc<Self>, root: PathBuf, f: impl FnOnce() -> T) -> T {
        let issuer = Arc::clone(self);
        speccoder_core::step::signing(
            std::rc::Rc::new(move |argvs: &[Vec<String>], interactive: bool| {
                issuer.mac(&root, argvs, interactive)
            }),
            f,
        )
    }
}

#[cfg(test)]
mod tests {
    use speccoder_core::step::{How, Step, person};

    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|w| (*w).to_owned()).collect()
    }

    fn token_of(step: &Step) -> String {
        match &step.how {
            How::Command { token: Some(t), .. } => t.clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn only_what_it_signed_runs() {
        let issuer = Arc::new(Issuer::new().unwrap_or_else(|| panic!("no random source")));
        let root = Path::new("/p");
        let step = issuer.signing(root.to_path_buf(), || person(vec![s(&["npm", "install"])]));
        let token = token_of(&step);
        assert_eq!(token.len(), 64);
        assert!(issuer.issued(root, &[s(&["npm", "install"])], false, &token));
        // Changed argvs, another project (a host bound elsewhere), the
        // keyboard flag flipped, another host's key, no token.
        assert!(!issuer.issued(root, &[s(&["npm", "install", "evil"])], false, &token));
        assert!(!issuer.issued(Path::new("/q"), &[s(&["npm", "install"])], false, &token));
        assert!(!issuer.issued(root, &[s(&["npm", "install"])], true, &token));
        let other = Issuer::new().unwrap_or_else(|| panic!("no random source"));
        assert!(!other.issued(root, &[s(&["npm", "install"])], false, &token));
        assert!(!issuer.issued(root, &[s(&["npm", "install"])], false, ""));
        assert!(!issuer.issued(root, &[], false, &issuer.mac(root, &[], false)));
        // A sign-in is signed as one: it cannot pass as a command to run here.
        let login = issuer.signing(root.to_path_buf(), || {
            person(vec![s(&["copilot", "login"])])
        });
        let t = token_of(&login);
        assert!(issuer.issued(root, &[s(&["copilot", "login"])], true, &t));
        assert!(!issuer.issued(root, &[s(&["copilot", "login"])], false, &t));
    }

    #[test]
    fn json_from_elsewhere_is_never_signed() {
        let issuer = Arc::new(Issuer::new().unwrap_or_else(|| panic!("no random source")));
        let v = issuer.signing(PathBuf::from("/p"), || {
            serde_json::from_str::<serde_json::Value>(
                r#"{"how":"command","argvs":[["rm","-rf","/"]],"interactive":false}"#,
            )
            .unwrap_or_default()
        });
        assert!(v.get("token").is_none());
        assert!(!format!("{issuer:?}").contains(&format!("{:?}", issuer.key)));
    }
}
