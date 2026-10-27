//! `speccoder attest verify`: re-checks every task's attestation against
//! the commit it is about — signatures with the signer's own tool, against
//! the verifier's trust, never the project's.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;
use speccoder_core::{
    AttestReport, CommandData, CommandResult, CommandStatus, ObjectVerification, SignerView,
    VerifyOutcome, VerifyReason,
};

use super::dsse::{self, Envelope};
use super::{notes, statement};
use crate::git::Git;

/// Where trust comes from: the verifier's, never the project's.
#[derive(Debug, Clone, Default)]
pub struct Trust {
    /// SSH allowed-signers file (`--allowed-signers`), else git's
    /// `gpg.ssh.allowedSignersFile`.
    pub allowed_signers: Option<PathBuf>,
    /// The Sigstore certificate identity expected.
    pub identity: Option<String>,
    /// The Sigstore OIDC issuer expected.
    pub issuer: Option<String>,
}

/// One object to verify.
#[derive(Debug, Clone)]
pub struct Target {
    /// The commit (or skip record) id.
    pub object: String,
    /// `task` or `skip`.
    pub kind: &'static str,
    /// Task ID.
    pub task: String,
}

fn reason(code: &str) -> VerifyReason {
    VerifyReason {
        code: code.to_owned(),
        field: None,
        expected: None,
        found: None,
    }
}

fn differs(code: &str, field: &str, expected: &str, found: &str) -> VerifyReason {
    VerifyReason {
        code: code.to_owned(),
        field: Some(field.to_owned()),
        expected: Some(expected.to_owned()),
        found: Some(found.to_owned()),
    }
}

/// A signature check's result.
enum Checked {
    Good(SignerView),
    Untrusted(SignerView),
    Bad(String),
    /// The signer's tool cannot be spawned: nothing can be said.
    ToolMissing(&'static str),
}

fn run(mut cmd: Command, input: Option<&[u8]>) -> Option<(bool, String, String)> {
    let mut child = cmd
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    if let (Some(data), Some(mut stdin)) = (input, child.stdin.take()) {
        let _ = stdin.write_all(data);
    }
    let out = child.wait_with_output().ok()?;
    Some((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

fn check_ssh(armored: &[u8], data: &[u8], keyid: &str, allowed: Option<&Path>) -> Checked {
    let Ok(dir) = tempfile::tempdir() else {
        return Checked::Bad("no temporary directory".to_owned());
    };
    let sig = dir.path().join("line.sig");
    if std::fs::write(&sig, armored).is_err() {
        return Checked::Bad("cannot write the signature".to_owned());
    }
    let view = |principal: Option<String>| SignerView {
        kind: "ssh".to_owned(),
        keyid: keyid.to_owned(),
        principal,
    };
    if let Some(allowed) = allowed {
        let mut find = Command::new("ssh-keygen");
        find.args(["-Y", "find-principals", "-s"])
            .arg(&sig)
            .arg("-f")
            .arg(allowed);
        if let Some((true, out, _)) = run(find, None)
            && let Some(principal) = out.lines().next().map(str::to_owned)
        {
            let mut verify = Command::new("ssh-keygen");
            verify
                .args(["-Y", "verify", "-f"])
                .arg(allowed)
                .args(["-I", &principal, "-n", super::sign::NAMESPACE, "-s"])
                .arg(&sig);
            return match run(verify, Some(data)) {
                Some((true, ..)) => Checked::Good(view(Some(principal))),
                Some((false, _, err)) => Checked::Bad(err.trim().to_owned()),
                None => Checked::ToolMissing("ssh-keygen"),
            };
        }
    }
    // Nobody the verifier trusts: is it at least a valid signature?
    let mut check = Command::new("ssh-keygen");
    check
        .args(["-Y", "check-novalidate", "-n", super::sign::NAMESPACE, "-s"])
        .arg(&sig);
    match run(check, Some(data)) {
        Some((true, ..)) => Checked::Untrusted(view(None)),
        Some((false, _, err)) => Checked::Bad(err.trim().to_owned()),
        None => Checked::ToolMissing("ssh-keygen"),
    }
}

fn check_openpgp(armored: &[u8], data: &[u8], keyid: &str) -> Checked {
    let Ok(dir) = tempfile::tempdir() else {
        return Checked::Bad("no temporary directory".to_owned());
    };
    let sig = dir.path().join("line.asc");
    let body = dir.path().join("line.pae");
    if std::fs::write(&sig, armored).is_err() || std::fs::write(&body, data).is_err() {
        return Checked::Bad("cannot write the signature".to_owned());
    }
    let mut cmd = Command::new("gpg");
    cmd.args(["--batch", "--status-fd", "1", "--verify"])
        .arg(&sig)
        .arg(&body);
    let Some((ok, status, _)) = run(cmd, None) else {
        return Checked::ToolMissing("gpg");
    };
    let view = |fingerprint: Option<String>, uid: Option<String>| SignerView {
        kind: "openpgp".to_owned(),
        keyid: fingerprint.unwrap_or_else(|| keyid.to_owned()),
        principal: uid,
    };
    match gpg_status(&status, ok) {
        Gpg::Bad => Checked::Bad("gpg: BAD signature".to_owned()),
        Gpg::Good { fingerprint, uid } => Checked::Good(view(Some(fingerprint), uid)),
        Gpg::Untrusted { fingerprint, uid } => Checked::Untrusted(view(fingerprint, uid)),
    }
}

/// What gpg's `--status-fd` lines say about a detached signature.
#[derive(Debug, PartialEq, Eq)]
enum Gpg {
    /// A signature that does not verify.
    Bad,
    /// One good signature by a key the keyring trusts fully or ultimately.
    Good {
        /// The signing key's fingerprint (`VALIDSIG`).
        fingerprint: String,
        /// The user ID `GOODSIG` names (the signer's own text).
        uid: Option<String>,
    },
    /// Anything else: no key here, an expired or revoked key, a key not
    /// trusted, or status lines that do not add up to one good signature.
    Untrusted {
        /// The signing key's fingerprint, when gpg gave one.
        fingerprint: Option<String>,
        /// The user ID, when gpg gave one.
        uid: Option<String>,
    },
}

/// Reads gpg's status lines exactly: a line counts only when it starts with
/// `[GNUPG:] ` and its keyword (the first token after) is the one asked for
/// — never a substring, since the user ID on `GOODSIG` is the signer's text.
/// Good takes gpg's success, exactly one `GOODSIG`, one `VALIDSIG` whose
/// fingerprint ends in that `GOODSIG`'s key ID, and one trust line that is
/// `TRUST_FULLY` or `TRUST_ULTIMATE`, with no bad, erroneous, expired or
/// revoked signature beside them.
fn gpg_status(status: &str, exited_ok: bool) -> Gpg {
    let mut good: Vec<(&str, Option<&str>)> = Vec::new();
    let mut valid: Vec<&str> = Vec::new();
    let mut trust: Vec<&str> = Vec::new();
    let mut spoiled = false;
    for line in status.lines() {
        let Some(rest) = line.strip_prefix("[GNUPG:] ") else {
            continue;
        };
        let (keyword, args) = rest.split_once(' ').unwrap_or((rest, ""));
        match keyword {
            "BADSIG" => return Gpg::Bad,
            "GOODSIG" => {
                let (id, uid) = args
                    .split_once(' ')
                    .map_or((args, None), |(id, uid)| (id, Some(uid)));
                good.push((id, uid));
            }
            "VALIDSIG" => valid.push(args.split(' ').next().unwrap_or_default()),
            "TRUST_UNDEFINED" | "TRUST_NEVER" | "TRUST_MARGINAL" | "TRUST_FULLY"
            | "TRUST_ULTIMATE" => trust.push(keyword),
            "ERRSIG" | "EXPSIG" | "EXPKEYSIG" | "REVKEYSIG" | "NO_PUBKEY" => spoiled = true,
            _ => {}
        }
    }
    let uid = good.first().and_then(|(_, u)| u.map(str::to_owned));
    let fingerprint = valid
        .first()
        .filter(|f| !f.is_empty())
        .map(|f| (*f).to_owned());
    let one_good = match (&good[..], &valid[..], &trust[..]) {
        ([(id, _)], [fpr], [level]) => {
            !id.is_empty()
                && fpr.len() >= id.len()
                && fpr.to_ascii_uppercase().ends_with(&id.to_ascii_uppercase())
                && matches!(*level, "TRUST_FULLY" | "TRUST_ULTIMATE")
        }
        _ => false,
    };
    match fingerprint {
        Some(fingerprint) if exited_ok && one_good && !spoiled => Gpg::Good { fingerprint, uid },
        fingerprint => Gpg::Untrusted { fingerprint, uid },
    }
}

fn check_sigstore(line: &Envelope, data: &[u8], trust: &Trust) -> Checked {
    let view = SignerView {
        kind: "sigstore".to_owned(),
        keyid: "sigstore".to_owned(),
        principal: trust.identity.clone(),
    };
    let (Some(material), Some(identity), Some(issuer)) =
        (&line.sigstore, &trust.identity, &trust.issuer)
    else {
        return Checked::Untrusted(view);
    };
    let Ok(dir) = tempfile::tempdir() else {
        return Checked::Bad("no temporary directory".to_owned());
    };
    let blob = dir.path().join("line.pae");
    let bundle = dir.path().join("line.sigstore.json");
    if std::fs::write(&blob, data).is_err()
        || std::fs::write(&bundle, material.to_string()).is_err()
    {
        return Checked::Bad("cannot write the bundle".to_owned());
    }
    let mut cmd = Command::new("cosign");
    cmd.args(["verify-blob", "--bundle"])
        .arg(&bundle)
        .args([
            "--certificate-identity",
            identity,
            "--certificate-oidc-issuer",
            issuer,
        ])
        .arg(&blob);
    match run(cmd, None) {
        Some((true, ..)) => Checked::Good(view),
        Some((false, _, err)) => Checked::Bad(err.trim().to_owned()),
        None => Checked::ToolMissing("cosign"),
    }
}

fn check_line(line: &Envelope, trust: &Trust) -> Option<Checked> {
    let sig = line.signatures.first()?;
    let Some(data) = line.signed_bytes() else {
        return Some(Checked::Bad("the payload is not base64".to_owned()));
    };
    if line.sigstore.is_some() {
        return Some(check_sigstore(line, &data, trust));
    }
    let Ok(armored) = STANDARD.decode(&sig.sig) else {
        return Some(Checked::Bad("the signature is not base64".to_owned()));
    };
    let text = String::from_utf8_lossy(&armored);
    Some(if text.contains("BEGIN SSH SIGNATURE") {
        check_ssh(
            &armored,
            &data,
            &sig.keyid,
            trust.allowed_signers.as_deref(),
        )
    } else if text.contains("BEGIN PGP SIGNATURE") {
        check_openpgp(&armored, &data, &sig.keyid)
    } else {
        Checked::Bad("an unknown signature format".to_owned())
    })
}

/// The commit's trailers by key.
fn trailers(repo: &Path, object: &str) -> BTreeMap<String, String> {
    let text = Git::at(repo)
        .run(&["log", "-1", "--format=%(trailers:only,unfold)", object])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    text.lines()
        .filter_map(|l| l.split_once(": "))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect()
}

/// The task line's hash in the tasks file at the commit, with its box read
/// as open (the hash a statement names was taken before the tick).
fn line_hash_at(cx: &Context<'_>, object: &str, tasks_path: &str, task: &str) -> Option<String> {
    let text = Git::at(cx.repo)
        .run(&["show", &format!("{object}:{tasks_path}")])
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())?;
    let format = cx.format.unwrap_or_else(|| crate::format::at(cx.repo));
    let (parsed, _) = format.parse_tasks(tasks_path, &text);
    let t = parsed.tasks.iter().find(|t| t.id == task)?;
    let line = text
        .lines()
        .nth(usize::try_from(t.line).ok()?.checked_sub(1)?)?;
    Some(crate::integrity::sha256_hex(&crate::integrity::untick(
        line.as_bytes(),
    )))
}

/// Everything that decides an object's outcome, given the trust input.
pub struct Context<'a> {
    /// The repository.
    pub repo: &'a Path,
    /// The feature.
    pub feature: &'a str,
    /// The tasks file's path in the repository, when tracked.
    pub tasks_path: Option<&'a str>,
    /// `speccoder.toml`'s path in the repository.
    pub config_path: &'a str,
    /// The verifier's trust.
    pub trust: &'a Trust,
    /// The format reading the task lists, when the caller knows it (read
    /// from git objects); `None`: the one of the checkout at `repo`.
    pub format: Option<&'static dyn crate::format::Adapter>,
}

/// Verifies one object.
#[must_use]
#[allow(clippy::too_many_lines)] // one invariant after another, in contract order
pub fn object(cx: &Context<'_>, target: &Target) -> ObjectVerification {
    let mut out = ObjectVerification {
        object: target.object.clone(),
        kind: target.kind.to_owned(),
        task: target.task.clone(),
        verdict: None,
        outcome: VerifyOutcome::Ok,
        reasons: Vec::new(),
        signer: None,
        statements: 0,
        same_tree_as: None,
    };
    let Some(note) = notes::read(cx.repo, &target.object) else {
        out.outcome = VerifyOutcome::Missing;
        out.reasons.push(reason("no_note"));
        out.same_tree_as = same_tree(cx.repo, &target.object);
        return out;
    };
    let mut worst = VerifyOutcome::Ok;
    let raise = |worst: &mut VerifyOutcome, o: VerifyOutcome| {
        if o.rank() > worst.rank() {
            *worst = o;
        }
    };
    let (lines, unreadable) = dsse::decode(&note);
    out.statements = lines.len();
    if unreadable > 0 || lines.is_empty() {
        raise(&mut worst, VerifyOutcome::Malformed);
        out.reasons.push(reason("unreadable_line"));
    }
    let mut parsed: Vec<(Value, String, &Envelope)> = Vec::new();
    for line in &lines {
        match (line.statement(), line.body()) {
            (Some(s), Some(body)) if s["_type"] == statement::STATEMENT => {
                parsed.push((s, crate::integrity::sha256_hex(&body), line));
            }
            _ => {
                raise(&mut worst, VerifyOutcome::Malformed);
                out.reasons.push(reason("not_a_statement"));
            }
        }
    }
    // 1. Every statement is about this object.
    for (s, ..) in &parsed {
        let found = s["subject"][0]["digest"]["gitCommit"]
            .as_str()
            .unwrap_or_default();
        if found != target.object {
            raise(&mut worst, VerifyOutcome::Mismatch);
            out.reasons.push(differs(
                "subject",
                "subject.digest.gitCommit",
                &target.object,
                found,
            ));
        }
    }
    // 2. Signatures, with the signer's own tool.
    let mut unsigned = false;
    // Each line's check is its own process; they run side by side.
    let checks: Vec<Option<Checked>> = std::thread::scope(|scope| {
        let handles: Vec<_> = parsed
            .iter()
            .map(|(_, _, line)| scope.spawn(move || check_line(line, cx.trust)))
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join().unwrap_or_else(|_| {
                    Some(Checked::Bad("the signature check panicked".to_owned()))
                })
            })
            .collect()
    });
    // A line a well-signed summary names by digest is covered by that
    // signature (a keyless Sigstore tick signs only its summary: one login).
    let covered: BTreeSet<String> = parsed
        .iter()
        .zip(&checks)
        .filter(|((s, ..), c)| {
            s["predicateType"] == statement::VSA && matches!(c, Some(Checked::Good(_)))
        })
        .flat_map(|((s, ..), _)| {
            s["predicate"]["inputAttestations"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|i| i["digest"]["sha256"].as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .collect();
    for ((_, digest, _), checked) in parsed.iter().zip(checks) {
        match checked {
            None if covered.contains(digest) => {}
            None => unsigned = true,
            Some(Checked::Good(v)) => {
                out.signer.get_or_insert(v);
            }
            Some(Checked::Untrusted(v)) => {
                raise(&mut worst, VerifyOutcome::UntrustedSigner);
                out.reasons.push(reason("untrusted_signer"));
                out.signer.get_or_insert(v);
            }
            Some(Checked::Bad(why)) => {
                raise(&mut worst, VerifyOutcome::BadSignature);
                let mut r = reason("signature");
                r.found = Some(why);
                out.reasons.push(r);
            }
            Some(Checked::ToolMissing(tool)) => {
                raise(&mut worst, VerifyOutcome::ToolMissing);
                let mut r = reason("tool_missing");
                r.found = Some(format!(
                    "{tool} cannot be run: install it to check this signature"
                ));
                out.reasons.push(r);
            }
        }
    }
    // 3–4. The task-verification agrees with the commit.
    let tvs: Vec<&Value> = parsed
        .iter()
        .filter(|(s, ..)| s["predicateType"] == statement::TASK_VERIFICATION)
        .map(|(s, ..)| &s["predicate"])
        .collect();
    let Some(tv) = tvs.first() else {
        raise(&mut worst, VerifyOutcome::Mismatch);
        out.reasons.push(reason("no_task_verification"));
        out.outcome = worst;
        return out;
    };
    let verdict = tv["verdict"].as_str().unwrap_or_default().to_owned();
    out.verdict = Some(verdict.clone());
    let tr = trailers(cx.repo, &target.object);
    let want = |key: &str, found: &str, out: &mut ObjectVerification, worst: &mut VerifyOutcome| {
        let expected = tr.get(key).map_or("", String::as_str);
        if expected != found {
            raise(worst, VerifyOutcome::Mismatch);
            out.reasons.push(differs("trailer", key, expected, found));
        }
    };
    let task_id = tv["task"]["id"].as_str().unwrap_or_default();
    if target.kind == "task" {
        want(
            "Spec-Task",
            &format!(
                "{}/{task_id}",
                tv["unit"]["feature"].as_str().unwrap_or_default()
            ),
            &mut out,
            &mut worst,
        );
        want("Verdict", &verdict, &mut out, &mut worst);
        // Why a regression-only tick is one: the trailer and the signed
        // reason agree (a tick from before reasons has neither).
        let regression = serde_json::from_value::<speccoder_core::regression::RegressionReason>(
            tv["regression"].clone(),
        )
        .ok();
        if regression.is_some() || tr.contains_key(speccoder_core::regression::TRAILER) {
            want(
                speccoder_core::regression::TRAILER,
                &regression
                    .as_ref()
                    .map(speccoder_core::regression::trailer)
                    .unwrap_or_default(),
                &mut out,
                &mut worst,
            );
        }
        if let Some(run) = tv["run"].as_str() {
            want("Run", run, &mut out, &mut worst);
        }
        if verdict == "attested" {
            want(
                "Accepted-By",
                tv["decider"]["by"].as_str().unwrap_or_default(),
                &mut out,
                &mut worst,
            );
        }
    } else {
        want(
            "Spec-Task",
            &format!(
                "{}/{task_id}",
                tv["unit"]["feature"].as_str().unwrap_or_default()
            ),
            &mut out,
            &mut worst,
        );
        want(
            "Skipped-By",
            tv["decider"]["by"].as_str().unwrap_or_default(),
            &mut out,
            &mut worst,
        );
        if verdict != "skipped" {
            raise(&mut worst, VerifyOutcome::Mismatch);
            out.reasons
                .push(differs("verdict", "verdict", "skipped", &verdict));
        }
    }
    let tree = Git::at(cx.repo)
        .line(&["rev-parse", &format!("{}^{{tree}}", target.object)])
        .unwrap_or_default();
    if tv["tree"].as_str() != Some(tree.as_str()) {
        raise(&mut worst, VerifyOutcome::Mismatch);
        out.reasons.push(differs(
            "tree",
            "tree",
            &tree,
            tv["tree"].as_str().unwrap_or_default(),
        ));
    }
    if let Some(path) = cx.tasks_path
        && let Some(at) = line_hash_at(cx, &target.object, path, task_id)
    {
        let claimed = tv["task"]["line_hash"].as_str().unwrap_or_default();
        if claimed != at {
            raise(&mut worst, VerifyOutcome::Mismatch);
            out.reasons
                .push(differs("line_hash", "task.line_hash", &at, claimed));
        }
    }
    if let Some(section) = tv["spec"]["section"].as_str() {
        let claimed = tv["spec"]["section_hash"].as_str().unwrap_or_default();
        match section_hash_at(cx, &target.object, section, task_id) {
            Ok(now) if claimed == now => {}
            Ok(now) => {
                raise(&mut worst, VerifyOutcome::Mismatch);
                out.reasons
                    .push(differs("section_hash", "spec.section_hash", &now, claimed));
            }
            // A requirement that cannot be read at the commit is not one
            // the claim agrees with: never a check skipped.
            Err(why) => {
                raise(&mut worst, VerifyOutcome::Mismatch);
                let mut r = reason("section_hash");
                r.field = Some("spec.section_hash".to_owned());
                r.found = Some(why);
                out.reasons.push(r);
            }
        }
    }
    // 5–6. The summary names exactly the other lines; decisions have none.
    let vsas: Vec<&(Value, String, &Envelope)> = parsed
        .iter()
        .filter(|(s, ..)| s["predicateType"] == statement::VSA)
        .collect();
    let test_results = parsed
        .iter()
        .filter(|(s, ..)| s["predicateType"] == statement::TEST_RESULT)
        .count();
    if matches!(verdict.as_str(), "verified" | "regression_only") {
        match vsas.as_slice() {
            [(v, own, _)] => {
                let named: BTreeSet<String> = v["predicate"]["inputAttestations"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|i| i["digest"]["sha256"].as_str().map(str::to_owned))
                    .collect();
                let present: BTreeSet<String> = parsed
                    .iter()
                    .map(|(_, d, _)| d.clone())
                    .filter(|d| d != own)
                    .collect();
                if named != present {
                    raise(&mut worst, VerifyOutcome::Mismatch);
                    out.reasons.push(differs(
                        "bundle",
                        "inputAttestations",
                        &format!("{} lines", named.len()),
                        &format!("{} lines", present.len()),
                    ));
                }
                let level = v["predicate"]["verifiedLevels"][0]
                    .as_str()
                    .unwrap_or_default();
                let expected = if verdict == "verified" {
                    "SPECCODER_VERIFIED"
                } else {
                    "SPECCODER_REGRESSION_ONLY"
                };
                if level != expected {
                    raise(&mut worst, VerifyOutcome::Mismatch);
                    out.reasons
                        .push(differs("level", "verifiedLevels", expected, level));
                }
                let summary_policy = v["predicate"]["policy"]["digest"]["sha256"]
                    .as_str()
                    .unwrap_or_default();
                let claimed = tv["policy_sha256"].as_str().unwrap_or_default();
                if summary_policy != claimed {
                    raise(&mut worst, VerifyOutcome::Mismatch);
                    out.reasons
                        .push(differs("policy", "policy.digest", claimed, summary_policy));
                }
                if let Some(at) = policy_at(cx, &target.object)
                    && at != claimed
                {
                    raise(&mut worst, VerifyOutcome::Mismatch);
                    out.reasons
                        .push(differs("policy", "policy_sha256", &at, claimed));
                }
            }
            _ => {
                raise(&mut worst, VerifyOutcome::Mismatch);
                out.reasons
                    .push(differs("bundle", "vsa", "1", &vsas.len().to_string()));
            }
        }
    } else if !vsas.is_empty() || test_results > 0 {
        raise(&mut worst, VerifyOutcome::Mismatch);
        out.reasons.push(reason("decision_with_verification"));
    }
    if worst == VerifyOutcome::Ok && unsigned {
        worst = VerifyOutcome::Unsigned;
    }
    out.outcome = worst;
    out
}

/// The judge-config hash recomputed from `speccoder.toml` at the parent.
fn policy_at(cx: &Context<'_>, object: &str) -> Option<String> {
    let text = Git::at(cx.repo)
        .run(&["show", &format!("{object}^:{}", cx.config_path)])
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())?;
    super::policy::judge_config_sha256(&text).ok()
}

/// The requirement's text hash at the commit, through the format adapter
/// over the commit's tree: what the spec said when the task was ticked.
fn section_hash_at(
    cx: &Context<'_>,
    object: &str,
    _section: &str,
    task: &str,
) -> Result<String, String> {
    // Only what a spec format reads: the top level's files and dot
    // directories, the directories holding a format's intent, and the tree
    // holding the unit — never the source tree, whose size would make
    // every check slow. Written from git objects: nothing but git runs.
    let tops = match cx.tasks_path {
        Some(p) => {
            let mut tops = super::tree::intent_tops(cx.repo, object)?;
            if let Some(unit_top) = p.split('/').next()
                && !tops.iter().any(|t| t == unit_top)
            {
                tops.push(unit_top.to_owned());
            }
            Some(tops)
        }
        None => None,
    };
    let dir = super::tree::materialize(cx.repo, object, tops.as_deref())?;
    let format = crate::format::at(dir.path());
    let snap = format
        .inspect(dir.path(), Some(cx.feature))
        .ok_or_else(|| format!("the commit holds no unit {}", cx.feature))?;
    let bundle = format
        .bundle(&snap, task)
        .map_err(|e| format!("the commit's unit holds no task {task}: {e:?}"))?;
    Ok(crate::integrity::sha256_hex(bundle.story.as_bytes()))
}

/// Another noted commit with the same tree (a rebase lost the note).
fn same_tree(repo: &Path, object: &str) -> Option<String> {
    let git = Git::at(repo);
    let tree = git
        .line(&["rev-parse", &format!("{object}^{{tree}}")])
        .ok()?;
    notes::noted(repo).into_iter().find(|c| {
        c != object
            && git
                .line(&["rev-parse", &format!("{c}^{{tree}}")])
                .is_ok_and(|t| t == tree)
    })
}

/// The task commits to verify: `rev` or the range's commits carrying
/// `Spec-Task`, or every task commit on `speccoder/<feature>`.
pub fn targets(repo: &Path, feature: &str, rev: Option<&str>) -> Result<Vec<Target>, String> {
    let git = Git::at(repo);
    let spec = match rev {
        Some(r) if r.contains("..") => r.to_owned(),
        Some(r) => {
            let commit = git
                .line(&["rev-parse", "--verify", &format!("{r}^{{commit}}")])
                .map_err(|e| e.0)?;
            let task = trailers(repo, &commit)
                .get("Spec-Task")
                .and_then(|t| t.rsplit('/').next().map(str::to_owned))
                .unwrap_or_default();
            return Ok(vec![Target {
                object: commit,
                kind: "task",
                task,
            }]);
        }
        None => format!("speccoder/{feature}"),
    };
    let log = git
        .run(&[
            "log",
            "--format=%H%x1f%(trailers:key=Spec-Task,valueonly)%x1f%(trailers:key=Verdict,valueonly)%x1e",
            &spec,
        ])
        .map_err(|e| e.0)?;
    let mut out = Vec::new();
    for record in String::from_utf8_lossy(&log).split('\u{1e}') {
        let f: Vec<&str> = record.trim().split('\u{1f}').collect();
        let [commit, task, verdict] = f[..] else {
            continue;
        };
        let (task, verdict) = (task.trim(), verdict.trim());
        if let Some(id) = task.strip_prefix(&format!("{feature}/"))
            && !verdict.is_empty()
        {
            out.push(Target {
                object: commit.to_owned(),
                kind: "task",
                task: id.to_owned(),
            });
        }
    }
    out.reverse();
    // The feature's skips, by default: a skip is a decision on the record too.
    if rev.is_none() {
        let refs = git
            .run(&[
                "for-each-ref",
                "--format=%(objectname) %(refname)",
                &format!("{}/{feature}/", notes::SKIPPED),
            ])
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        for line in refs.lines() {
            if let Some((object, name)) = line.split_once(' ') {
                out.push(Target {
                    object: object.to_owned(),
                    kind: "skip",
                    task: name.rsplit('/').next().unwrap_or_default().to_owned(),
                });
            }
        }
    }
    Ok(out)
}

/// `speccoder attest verify` over `targets`.
#[must_use]
pub fn report(cx: &Context<'_>, targets: &[Target], require_signed: bool) -> CommandResult {
    // Objects are independent; a few at a time, in the targets' order.
    let width = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
    let objects: Vec<ObjectVerification> = targets
        .chunks(width.max(1))
        .flat_map(|chunk| {
            std::thread::scope(|scope| {
                let handles: Vec<_> = chunk
                    .iter()
                    .map(|t| scope.spawn(move || object(cx, t)))
                    .collect();
                handles
                    .into_iter()
                    .zip(chunk)
                    .map(|(h, t)| {
                        h.join().unwrap_or_else(|_| ObjectVerification {
                            object: t.object.clone(),
                            kind: t.kind.to_owned(),
                            task: t.task.clone(),
                            verdict: None,
                            outcome: VerifyOutcome::Malformed,
                            reasons: vec![reason("check_panicked")],
                            signer: None,
                            statements: 0,
                            same_tree_as: None,
                        })
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    // In the order the summary reads: ok first, then the worst first.
    const KINDS: [&str; 8] = [
        "ok",
        "missing",
        "malformed",
        "bad_signature",
        "mismatch",
        "tool_missing",
        "untrusted_signer",
        "unsigned",
    ];
    let mut totals: BTreeMap<String, usize> = KINDS.iter().map(|k| ((*k).to_owned(), 0)).collect();
    for o in &objects {
        *totals.entry(o.outcome.word().to_owned()).or_default() += 1;
    }
    let failed = objects.iter().any(|o| match o.outcome {
        VerifyOutcome::Ok => false,
        VerifyOutcome::Unsigned => require_signed,
        _ => true,
    });
    let summary: Vec<String> = KINDS
        .iter()
        .filter_map(|k| {
            totals
                .get(*k)
                .filter(|n| **n > 0)
                .map(|n| format!("{n} {k}"))
        })
        .collect();
    let message = format!(
        "{} task object(s): {}",
        objects.len(),
        if summary.is_empty() {
            "none".to_owned()
        } else {
            summary.join(", ")
        }
    );
    let status = if failed {
        CommandStatus::AttestationFailed
    } else {
        CommandStatus::Ok
    };
    CommandResult::new("attest", status, &message).with_data(CommandData::Attest(Box::new(
        AttestReport {
            feature: cx.feature.to_owned(),
            trust: serde_json::json!({
                "ssh_allowed_signers": cx.trust.allowed_signers.as_ref().map(|p| p.display().to_string()),
                "gpg": true,
                "sigstore": cx.trust.identity,
            }),
            objects,
            totals,
        },
    )))
}

/// `speccoder attest verify` for the project at `path`.
#[must_use]
pub fn verify_at(
    path: &Path,
    feature: Option<&str>,
    rev: Option<&str>,
    trust: &Trust,
    require_signed: bool,
) -> CommandResult {
    let inspected = crate::inspect(path, feature);
    let Some(CommandData::Inspect(snap)) = inspected.data() else {
        return CommandResult::carried("attest", &inspected);
    };
    let Some(f) = snap.feature.as_ref() else {
        return CommandResult::new(
            "attest",
            CommandStatus::SelectionNeeded,
            inspected.message(),
        );
    };
    let root = PathBuf::from(&snap.root);
    let git = Git::at(&root);
    let top = git
        .toplevel()
        .ok()
        .flatten()
        .unwrap_or_else(|| root.clone());
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let rel = canonical(&root)
        .strip_prefix(canonical(&top))
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let in_repo = |p: &str| rel.join(p).to_string_lossy().replace('\\', "/");
    let tasks_path = f.tasks.as_ref().map(|t| in_repo(&t.path));
    let config_path = crate::verify::find_config(&root)
        .and_then(|d| {
            canonical(&d)
                .strip_prefix(canonical(&top))
                .ok()
                .map(Path::to_path_buf)
        })
        .map_or_else(
            || crate::verify::config::FILE.to_owned(),
            |d| {
                d.join(crate::verify::config::FILE)
                    .to_string_lossy()
                    .replace('\\', "/")
            },
        );
    let mut trust = trust.clone();
    if trust.allowed_signers.is_none() {
        trust.allowed_signers = git
            .line(&["config", "gpg.ssh.allowedSignersFile"])
            .ok()
            .filter(|p| !p.is_empty())
            .map(|p| match (p.strip_prefix("~/"), std::env::var_os("HOME")) {
                (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
                _ => PathBuf::from(p),
            });
    }
    let targets = match targets(&top, &f.feature.name, rev) {
        Ok(t) => t,
        Err(e) => return CommandResult::new("attest", CommandStatus::GitPrecondition, &e),
    };
    if targets.is_empty() {
        return CommandResult::new(
            "attest",
            CommandStatus::NotFound,
            &format!("no ticked task of {} to verify", f.feature.name),
        );
    }
    let cx = Context {
        repo: &top,
        feature: &f.feature.name,
        tasks_path: tasks_path.as_deref(),
        config_path: &config_path,
        trust: &trust,
        format: None,
    };
    report(&cx, &targets, require_signed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FPR: &str = "0123456789ABCDEF0123456789ABCDEF01234567";

    fn status(uid: &str, trust: &str) -> String {
        format!(
            "[GNUPG:] NEWSIG\n[GNUPG:] KEY_CONSIDERED {FPR} 0\n[GNUPG:] SIG_ID abc 2026-10-26 1793000000\n\
             [GNUPG:] GOODSIG 89ABCDEF01234567 {uid}\n\
             [GNUPG:] VALIDSIG {FPR} 2026-10-26 1793000000 0 4 0 22 10 00 {FPR}\n\
             [GNUPG:] {trust} 0 pgp\n"
        )
    }

    #[test]
    fn a_fully_trusted_good_signature_is_good() {
        assert_eq!(
            gpg_status(&status("Ada <ada@example.com>", "TRUST_ULTIMATE"), true),
            Gpg::Good {
                fingerprint: FPR.to_owned(),
                uid: Some("Ada <ada@example.com>".to_owned()),
            }
        );
        assert!(matches!(
            gpg_status(&status("Ada <ada@example.com>", "TRUST_FULLY"), true),
            Gpg::Good { .. }
        ));
    }

    #[test]
    fn trust_words_in_the_signers_user_id_are_not_trust() {
        // The user ID is the signer's own text: a key nobody trusts that
        // calls itself TRUST_ULTIMATE is still a key nobody trusts.
        for uid in [
            "Eve TRUST_ULTIMATE <eve@example.com>",
            "Eve [GNUPG:] TRUST_FULLY 0 pgp",
            "TRUST_ULTIMATE",
        ] {
            assert!(
                matches!(
                    gpg_status(&status(uid, "TRUST_UNDEFINED"), true),
                    Gpg::Untrusted { .. }
                ),
                "{uid}"
            );
        }
        // A line the uid forges is a second trust line: not one good signature.
        let forged = status("Eve\n[GNUPG:] TRUST_ULTIMATE 0 pgp", "TRUST_UNDEFINED");
        assert!(matches!(gpg_status(&forged, true), Gpg::Untrusted { .. }));
    }

    #[test]
    fn only_one_good_signature_by_the_key_gpg_validated_is_good() {
        let s = status("Ada", "TRUST_ULTIMATE");
        // gpg failed, a bad signature, an expired key, no VALIDSIG, a
        // GOODSIG of another key than VALIDSIG's.
        assert!(matches!(gpg_status(&s, false), Gpg::Untrusted { .. }));
        assert_eq!(
            gpg_status(&format!("{s}[GNUPG:] BADSIG 89ABCDEF01234567 Ada\n"), true),
            Gpg::Bad
        );
        assert!(matches!(
            gpg_status(
                &format!("{s}[GNUPG:] EXPKEYSIG 89ABCDEF01234567 Ada\n"),
                true
            ),
            Gpg::Untrusted { .. }
        ));
        let no_valid: String = s
            .lines()
            .filter(|l| !l.contains("VALIDSIG"))
            .map(|l| format!("{l}\n"))
            .collect();
        assert!(matches!(gpg_status(&no_valid, true), Gpg::Untrusted { .. }));
        let other = s.replace("GOODSIG 89ABCDEF01234567", "GOODSIG FFFFFFFFFFFFFFFF");
        assert!(matches!(gpg_status(&other, true), Gpg::Untrusted { .. }));
        // Not at a line's start: not a status line.
        let indented = s.replace("[GNUPG:] TRUST_ULTIMATE", "x [GNUPG:] TRUST_ULTIMATE");
        assert!(matches!(gpg_status(&indented, true), Gpg::Untrusted { .. }));
    }
}
