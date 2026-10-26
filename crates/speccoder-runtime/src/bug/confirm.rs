//! The one confirmation: bound to the digest of what the person was shown,
//! recorded with who, through what and on what basis, and turned into the
//! fix's pre-frozen acceptance set. A changed triad lapses it.

use std::path::Path;

use speccoder_core::acceptance::{Decided, PreFrozenAcceptance, Producer, VisibleTest};
use speccoder_core::actions::Via;
use speccoder_core::bug::{
    AcceptanceRef, Basis, BugReport, Confirmation, Outcome, PreFrozenRef, Reproduction, Stage,
};

use crate::integrity::sha256_hex;

/// Unix seconds now.
#[must_use]
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `sha256:<hex>` over the reproductions shown: each statement, file and
/// content digest, sorted.
#[must_use]
pub fn digest(reproductions: &[Reproduction]) -> String {
    let mut lines: Vec<String> = reproductions
        .iter()
        .map(|r| format!("{}\t{}\t{}", r.statement, r.file, r.digest))
        .collect();
    lines.sort();
    format!("sha256:{}", sha256_hex(lines.join("\n").as_bytes()))
}

/// Records the confirmation of `bug`'s reproductions and stores them as the
/// fix's pre-frozen set; returns the record and the files.
pub fn freeze(
    state: &Path,
    bug: &mut BugReport,
    by: &str,
    via: Via,
    basis: Basis,
) -> Result<(PreFrozenAcceptance, Vec<(String, String)>), String> {
    let digest = bug
        .confirm_digest
        .clone()
        .ok_or_else(|| "there are no reproductions to confirm".to_owned())?;
    let paths: Vec<String> = bug.reproductions.iter().map(|r| r.file.clone()).collect();
    let files = super::state::repro(state, &bug.unit, &bug.task, &paths);
    if files.len() != paths.len() {
        return Err("the reproductions' contents are missing from the state directory".to_owned());
    }
    let at = now();
    let record = PreFrozenAcceptance {
        unit: bug.unit.clone(),
        requirement: bug.requirement.clone(),
        wording_digest: bug.triad.digest.clone(),
        approval_digest: digest.clone(),
        visible: bug
            .reproductions
            .iter()
            .map(|r| VisibleTest {
                path: r.file.clone(),
                sha256: r.digest.clone(),
                cites: vec![r.statement.clone()],
            })
            .collect(),
        held_out_sealed: bug.held_out_sealed.clone(),
        producer: if basis == Basis::NamedTest {
            Producer::PersonTest
        } else {
            Producer::BugConfirmation
        },
        decided: Decided {
            by: by.to_owned(),
            via,
            at,
        },
    };
    crate::pre_frozen::store(state, &record, &files)?;
    bug.confirmation = Some(Confirmation {
        digest: digest.clone(),
        triad_digest: bug.triad.digest.clone(),
        by: by.to_owned(),
        via,
        basis,
        at,
    });
    bug.acceptance = Some(AcceptanceRef {
        source: record.producer.source(),
        pre_frozen: PreFrozenRef {
            manifest: crate::pre_frozen::manifest_digest(&record),
            approval: digest,
            wording: record.wording_digest.clone(),
        },
    });
    bug.stage = Stage::Fix;
    bug.outcome = Outcome::Recorded;
    bug.code = None;
    bug.handoff = None;
    super::state::save(state, bug)?;
    Ok((record, files))
}

/// Forgets a confirmation the triad no longer stands for: the bug goes
/// back to recorded, its reproductions to be written again.
pub fn lapse(
    state: &Path,
    bug: &mut BugReport,
    triad: speccoder_core::bug::Triad,
) -> Result<(), String> {
    crate::pre_frozen::remove(state, &bug.unit, &bug.requirement);
    bug.triad = triad;
    bug.confirmation = None;
    bug.confirm_digest = None;
    bug.acceptance = None;
    bug.reproductions
        .retain(|r| r.source == speccoder_core::bug::Source::Person);
    bug.held_out.clear();
    bug.held_out_sealed = None;
    bug.tries.clear();
    bug.stage = Stage::Reproduce;
    bug.outcome = Outcome::Recorded;
    super::state::save(state, bug)
}
