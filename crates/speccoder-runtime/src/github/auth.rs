//! Who speccoder is on GitHub (F-089): a token from the environment (CI:
//! `GH_TOKEN`, `GITHUB_TOKEN`; an Enterprise host: `GH_ENTERPRISE_TOKEN`),
//! else the one `speccoder auth login` kept in the operating system's
//! credential store (service `speccoder`, account `github:<host>`). Never a
//! file; never printed.

use std::fmt;

/// The credential-store service every token is filed under.
pub const SERVICE: &str = "speccoder";

/// What a token needs: `repo` to read issues and open pull requests on a
/// private repository, `read:org` to see team review requests. A
/// fine-grained token needs Issues (read), Pull requests (read and write),
/// Contents and Metadata (read).
pub const SCOPES: &str = "repo read:org";

/// A token: redacted wherever it is printed.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wraps a token.
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into().trim().to_owned())
    }

    /// The value, for the one place it goes: a header to its own host, or
    /// the credential store.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([redacted])")
    }
}

/// Where a token came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// An environment variable (its name).
    Env(&'static str),
    /// The operating system's credential store.
    Keychain,
}

impl Source {
    /// `GH_TOKEN`, `the keychain`.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Env(v) => v,
            Self::Keychain => "the keychain",
        }
    }
}

fn account(host: &str) -> String {
    format!("github:{host}")
}

fn entry(host: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, &account(host)).map_err(|e| store_error(&e))
}

fn store_error(e: &keyring::Error) -> String {
    format!(
        "the credential store is not available ({e}); set GH_TOKEN instead — speccoder keeps no token in a file"
    )
}

/// The token for `host`, and where it came from.
#[must_use]
pub fn token(host: &str) -> Option<(Secret, Source)> {
    let vars: &[&'static str] = if host == "github.com" {
        &["GH_TOKEN", "GITHUB_TOKEN"]
    } else {
        &["GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN"]
    };
    for v in vars {
        if let Some(t) = std::env::var(v).ok().filter(|t| !t.trim().is_empty()) {
            return Some((Secret::new(t), Source::Env(v)));
        }
    }
    // A machine without a person never reaches the store.
    if std::env::var_os("SPECCODER_NO_KEYCHAIN").is_some() {
        return None;
    }
    entry(host)
        .ok()?
        .get_password()
        .ok()
        .filter(|t| !t.trim().is_empty())
        .map(|t| (Secret::new(t), Source::Keychain))
}

/// Keeps `token` for `host` in the credential store.
///
/// # Errors
///
/// No credential store here.
pub fn store(host: &str, token: &Secret) -> Result<(), String> {
    entry(host)?
        .set_password(token.expose())
        .map_err(|e| store_error(&e))
}

/// Forgets `host`'s token; `Ok(false)`: there was none.
///
/// # Errors
///
/// No credential store here.
pub fn forget(host: &str) -> Result<bool, String> {
    match entry(host)?.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(store_error(&e)),
    }
}

/// speccoder's GitHub App (F-092): its Client ID (public by design — it
/// names the app, it proves nothing) and its slug. Repository permissions:
/// Contents, Checks, Commit statuses, Metadata read; Issues and Pull requests
/// read and write; user tokens that do not expire; the device flow.
pub const APP_CLIENT_ID: &str = "Iv23liZofBDOUS5F5z2g";

/// `https://github.com/apps/speccoder-app`.
pub const APP_SLUG: &str = "speccoder-app";

/// Where a repository's owner installs speccoder's app.
#[must_use]
pub fn install_url() -> String {
    format!("https://github.com/apps/{APP_SLUG}/installations/new")
}

/// The app the device flow signs in with on `host`:
/// `SPECCODER_GITHUB_CLIENT_ID` (an Enterprise server's own app, or another
/// one), else speccoder's on github.com; none on another host.
#[must_use]
pub fn client_id(host: &str) -> Option<String> {
    std::env::var("SPECCODER_GITHUB_CLIENT_ID")
        .ok()
        .filter(|c| !c.trim().is_empty())
        .or_else(|| (host == "github.com").then(|| APP_CLIENT_ID.to_owned()))
}

/// A device-flow sign-in waiting for the person: the code to type, where.
#[derive(Debug, Clone)]
pub struct Pending {
    /// What the person types at `verification_uri`.
    pub user_code: String,
    /// `https://github.com/login/device`.
    pub verification_uri: String,
    device_code: Secret,
    interval: u64,
    expires_in: u64,
}

/// Starts the device flow on `oauth` (`https://github.com`).
///
/// # Errors
///
/// No client id, or the host refused.
pub fn start(oauth: &str, client_id: &str) -> Result<Pending, String> {
    let v = super::http::form(
        &format!("{oauth}/login/device/code"),
        &[("client_id", client_id), ("scope", SCOPES)],
    )?;
    let s = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
    if s("device_code").is_empty() {
        return Err(format!("the device flow did not start: {v}"));
    }
    Ok(Pending {
        user_code: s("user_code"),
        verification_uri: s("verification_uri"),
        device_code: Secret::new(s("device_code")),
        interval: v["interval"].as_u64().unwrap_or(5).max(1),
        expires_in: v["expires_in"].as_u64().unwrap_or(900),
    })
}

/// Waits until the person entered the code (or it expired): the token.
///
/// # Errors
///
/// Expired, denied, or the host refused.
pub fn wait(oauth: &str, client_id: &str, p: &Pending) -> Result<Secret, String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(p.expires_in);
    let mut interval = p.interval;
    while std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_secs(interval));
        let v = super::http::form(
            &format!("{oauth}/login/oauth/access_token"),
            &[
                ("client_id", client_id),
                ("device_code", p.device_code.expose()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ],
        )?;
        if let Some(t) = v["access_token"].as_str().filter(|t| !t.is_empty()) {
            return Ok(Secret::new(t));
        }
        match v["error"].as_str().unwrap_or_default() {
            "authorization_pending" => {}
            "slow_down" => interval += 5,
            "expired_token" => break,
            "access_denied" => return Err("the sign-in was denied".to_owned()),
            other => return Err(format!("the sign-in failed: {other}")),
        }
    }
    Err("the code expired before it was entered; sign in again".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_never_prints() {
        let s = Secret::new(" ghp_secret \n");
        assert_eq!(s.expose(), "ghp_secret");
        assert_eq!(format!("{s:?}"), "Secret([redacted])");
    }
}
