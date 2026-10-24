//! Signing statements with the person's git signing key, as git would:
//! `ssh-keygen -Y sign` for `gpg.format = ssh`, `gpg --detach-sign` for
//! OpenPGP; `cosign` for opt-in Sigstore. The namespace `in-toto` keeps a
//! statement signature from ever passing as a commit signature.
//!
//! Signing never fails a tick: whatever goes wrong leaves the statement
//! unsigned, with the reason (`attest sign` can sign it later).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

use super::dsse::Signature;
use crate::git::Git;
use crate::substrate::run_config::{AttestConfig, Signer as Configured};

/// The SSH signature namespace for statements.
pub const NAMESPACE: &str = "in-toto";

/// How statements get signed here.
#[derive(Debug)]
pub enum Signer {
    /// `ssh-keygen -Y sign` with this key file (a private key, or a public
    /// key whose private half is in the agent).
    Ssh {
        /// `gpg.ssh.program`, default `ssh-keygen`.
        program: String,
        /// The key file.
        key: PathBuf,
        /// Holds a `key::` literal's temporary file alive.
        _literal: Option<tempfile::NamedTempFile>,
    },
    /// `gpg --detach-sign --armor`, with this key (the default key when empty).
    OpenPgp {
        /// `gpg.openpgp.program`, `gpg.program`, default `gpg`.
        program: String,
        /// `user.signingKey`.
        key: String,
    },
    /// `cosign sign-blob`, keyless.
    Sigstore,
}

impl Signer {
    /// `ssh`, `openpgp` or `sigstore`.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Ssh { .. } => "ssh",
            Self::OpenPgp { .. } => "openpgp",
            Self::Sigstore => "sigstore",
        }
    }
}

fn config(repo: &Path, key: &str) -> Option<String> {
    Git::at(repo)
        .line(&["config", key])
        .ok()
        .filter(|v| !v.trim().is_empty())
}

/// The signer `[attest]` and git's configuration in `repo` (the person's
/// checkout) describe, or why statements stay unsigned.
pub fn signer(repo: &Path, attest: &AttestConfig) -> Result<Signer, String> {
    match attest.signer {
        Configured::None => Err("[attest] signer = \"none\"".to_owned()),
        Configured::Sigstore => {
            if which("cosign") {
                Ok(Signer::Sigstore)
            } else {
                Err("[attest] signer = \"sigstore\", but cosign is not installed".to_owned())
            }
        }
        Configured::Git => match config(repo, "gpg.format").as_deref() {
            Some("ssh") => ssh(repo),
            Some("x509") => Err("x509 signing keys are not supported".to_owned()),
            _ => {
                let key = config(repo, "user.signingKey").unwrap_or_default();
                if key.is_empty() && config(repo, "commit.gpgSign").is_none() {
                    return Err("no signing key is configured in git (user.signingKey)".to_owned());
                }
                Ok(Signer::OpenPgp {
                    program: config(repo, "gpg.openpgp.program")
                        .or_else(|| config(repo, "gpg.program"))
                        .unwrap_or_else(|| "gpg".to_owned()),
                    key,
                })
            }
        },
    }
}

fn ssh(repo: &Path) -> Result<Signer, String> {
    let program = config(repo, "gpg.ssh.program").unwrap_or_else(|| "ssh-keygen".to_owned());
    let key = match config(repo, "user.signingKey") {
        Some(k) => k,
        None => {
            let Some(cmd) = config(repo, "gpg.ssh.defaultKeyCommand") else {
                return Err("no SSH signing key is configured in git (user.signingKey)".to_owned());
            };
            let out = Command::new("sh")
                .args(["-c", &cmd])
                .current_dir(repo)
                .output()
                .map_err(|e| format!("gpg.ssh.defaultKeyCommand: {e}"))?;
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned()
        }
    };
    if let Some(literal) = key.strip_prefix("key::") {
        let mut file = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
        writeln!(file, "{literal}").map_err(|e| e.to_string())?;
        return Ok(Signer::Ssh {
            program,
            key: file.path().to_path_buf(),
            _literal: Some(file),
        });
    }
    let key = expand_home(&key);
    if !key.exists() {
        return Err(format!(
            "the SSH signing key {} does not exist",
            key.display()
        ));
    }
    Ok(Signer::Ssh {
        program,
        key,
        _literal: None,
    })
}

fn expand_home(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(p),
    }
}

fn which(program: &str) -> bool {
    Command::new(program)
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn piped(mut cmd: Command, input: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input).map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok((out.stdout, out.stderr))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// Signs `bytes` (a statement's PAE). The signature is base64 of the
/// signer's own armored output; `keyid` is the key's fingerprint.
pub fn sign(
    signer: &Signer,
    bytes: &[u8],
) -> Result<(Signature, Option<serde_json::Value>), String> {
    match signer {
        Signer::Ssh { program, key, .. } => {
            let mut cmd = Command::new(program);
            cmd.args(["-Y", "sign", "-n", NAMESPACE, "-f"]).arg(key);
            let (armored, _) = piped(cmd, bytes).map_err(|e| format!("ssh-keygen: {e}"))?;
            Ok((
                Signature {
                    keyid: ssh_fingerprint(program, key)?,
                    sig: STANDARD.encode(armored),
                },
                None,
            ))
        }
        Signer::OpenPgp { program, key } => {
            let mut cmd = Command::new(program);
            cmd.args(["--batch", "--status-fd", "2", "--detach-sign", "--armor"]);
            if !key.is_empty() {
                cmd.args(["-u", key]);
            }
            let (armored, status) = piped(cmd, bytes).map_err(|e| format!("gpg: {e}"))?;
            let fingerprint = String::from_utf8_lossy(&status)
                .lines()
                .find_map(|l| l.strip_prefix("[GNUPG:] SIG_CREATED "))
                .and_then(|l| l.split_whitespace().last())
                .map(str::to_owned)
                .unwrap_or_else(|| key.clone());
            Ok((
                Signature {
                    keyid: fingerprint,
                    sig: STANDARD.encode(armored),
                },
                None,
            ))
        }
        Signer::Sigstore => {
            let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
            let blob = dir.path().join("statement.pae");
            let bundle = dir.path().join("statement.sigstore.json");
            std::fs::write(&blob, bytes).map_err(|e| e.to_string())?;
            let out = Command::new("cosign")
                .args(["sign-blob", "--yes", "--bundle"])
                .arg(&bundle)
                .arg(&blob)
                .output()
                .map_err(|e| format!("cosign: {e}"))?;
            if !out.status.success() {
                return Err(format!(
                    "cosign: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
            let material: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&bundle).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("cosign bundle: {e}"))?;
            let sig = material["messageSignature"]["signature"]
                .as_str()
                .or_else(|| material["base64Signature"].as_str())
                .unwrap_or_default()
                .to_owned();
            Ok((
                Signature {
                    keyid: "sigstore".to_owned(),
                    sig,
                },
                Some(material),
            ))
        }
    }
}

/// `SHA256:…` of an SSH key file.
fn ssh_fingerprint(program: &str, key: &Path) -> Result<String, String> {
    let out = Command::new(program)
        .args(["-l", "-E", "sha256", "-f"])
        .arg(key)
        .output()
        .map_err(|e| e.to_string())?;
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .map(str::to_owned)
        .ok_or_else(|| "ssh-keygen -l printed no fingerprint".to_owned())
}
