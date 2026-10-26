+++
title = "The task-verification predicate"
description = "https://speccoder.dev/task-verification/v0 as an open specification: what speccoder signs about a ticked task, every field, its JSON Schema, the bundle it travels in, and the rules a verifier applies."
weight = 97

[extra]
group = "Trust"
+++

speccoder's in-toto predicate: *this task of this spec was verified, by this evidence, against the spec text its commit holds*. Anyone can check it, or write a verifier, without speccoder. The structure follows in-toto's [predicate template](https://github.com/in-toto/attestation/blob/main/spec/predicates/template.md).

## Predicate type and versioning

`predicateType`: **`https://speccoder.dev/task-verification/v0`**

Within v0 the predicate only grows: an added field raises **`revision`** (an integer; absent means 1). Removing a field or changing what one means is a new predicate type, `…/v1`.

- **Verifiers** MUST tolerate unknown fields and higher revisions within v0: keep the signed bytes as they are, ignore those fields for the outcome, and name them as not evaluated. A predicate of another major version is reported as malformed (`predicate …/v1 not supported by this verifier`), never guessed at.
- **Producers** MUST NOT make an [opaque field](#opaque-fields) required.

This page describes revision **2**.

## Purpose

A reviewer or a CI job checks that a ticked task (`[X]` in `tasks.md`, a task of an OpenSpec change) was verified against the spec its commit holds: which tests failed first and passed after, what regressed, how strong the tests are, who decided otherwise. Signatures are checked against the reviewer's own trust.

## Prerequisites

- [in-toto Statement v1](https://github.com/in-toto/attestation/blob/main/spec/v1/statement.md), each in a [DSSE](https://github.com/secure-systems-lab/dsse) envelope (`payloadType: application/vnd.in-toto+json`), signed over DSSE's pre-authentication encoding with SSH (`ssh-keygen -Y sign -n in-toto`), OpenPGP or Sigstore.
- The **bundle**: the envelopes as JSON Lines, kept as the git note of the task commit under `refs/notes/speccoder`.

## Model

Every statement of a tick has the same subject: `[{ "name": "<unit>/<task>", "digest": { "gitCommit": "<task commit>" } }]`. A tick's bundle holds:

| Line | Predicate type | |
|---|---|---|
| per test phase that ran | `https://in-toto.io/attestation/test-result/v0.1` | `configuration[0].name` is `speccoder:phase/baseline`, `red`, `green` or `held_out`; for `held_out`, its digest is the held-out set's manifest sha256 |
| one | `https://speccoder.dev/task-verification/v0` | this predicate |
| one | `https://slsa.dev/verification_summary/v1` | names every other line by sha256 (`inputAttestations`), the level `SPECCODER_VERIFIED` or `SPECCODER_REGRESSION_ONLY`, the judge configuration's digest as its policy |

A person's decision (`attested` by accept, `skipped` by skip) is one task-verification line alone, with `decider` and no test results or summary.

The task commit's trailers repeat what a reader needs without the note: `Spec-Task: <unit>/<task>`, `Verdict:`, and `Run:`, `Accepted-By:`, `Held-Out:` where they apply.

## Fields

The [JSON Schema](https://hupe1980.github.io/speccoder/schemas/task-verification-v0.json) is generated from the type speccoder serializes (`TaskVerificationV0`); every statement speccoder writes validates against it.

| Field | Type | Required | Meaning |
|---|---|---|---|
| `revision` | integer | no (absent: 1) | the revision within v0 |
| `verdict` | string | yes | `verified`, `regression_only`, `attested` or `skipped` |
| `unit` | `{format, feature, dir}` | yes | the spec format (`speckit`, `openspec`), the unit's name and directory |
| `task` | `{id, line_hash, title}` | yes | the task ID, the sha256 of its line with the box read as open (`[ ]`) before the tick, its title |
| `tree` | string | yes | the task commit's tree |
| `spec` | `{section, section_hash}` | ticks | the requirement or story the task belongs to and the sha256 of its text as the attempt was given it |
| `trace` | array | ticks | rows `{requirement, requirement_hash, phase, result}` for each requirement the title cites, and `{requirement, scenario, test, phase, result}` for each acceptance test |
| `integrity` | `{findings}` | ticks | the diff audit's findings |
| `monitor` | `{state, findings}` | ticks | the gaming monitor: `clean`, `suspected`, `unreviewed` or `off` |
| `mutation` | object or null | ticks | mutation testing: `caught`, `missed` |
| `strength` | object or null | ticks | the acceptance set's strength: `visible` and `combined` shares, near misses, permitted alternatives and rulings |
| `sandbox`, `executor`, `model`, `run` | object / null | ticks | where the attempt ran, who implemented, which model judged, the agentplane run |
| `decisions`, `instructions` | array | ticks | people's decisions on the attempt's calls; the repository instruction files given to it |
| `decider` | `{by, reason}` | decisions | who accepted or skipped, and why |
| `policy_sha256` | string | yes | sha256 of the judge's configuration in `speccoder.toml` at the parent commit |
| `time` | string | yes | RFC 3339, UTC |
| `red` | object | no (revision 2) | the red run's output: `kind` (`assertion`, `missing_symbol`), `excerpt` (its last ≤ 2048 bytes, or `null`), `excerpt_sha256`, `output_sha256`, `withheld` |

### Opaque fields

These fields are **optional and opaque** to a verifier: any JSON value, kept in the signed bytes, shown as recorded, never deciding an outcome.

| Field | Written by speccoder | Shape |
|---|---|---|
| `bug` | a bug task's tick | `{home, triad_sha256, outcome, reproductions[], special_case[], pass_to_pass}`; for a trailer-only task, `triad_sha256` is the anchor |
| `acceptance` | a bug task's tick, a kept bug test | `{source, pre_frozen}`; `source` is one of `authored`, `review`, `bug_confirmation`, `person_test`, `kept_regression_test` (speccoder writes the last three) |
| `context` | no (reserved) | what reached the model, by digest |
| `dependencies` | no (reserved) | dependencies the change added, with their registry checksums |

## Parsing and evaluation

A verifier applies, per ticked task (as [`speccoder-verify`](@/docs/verifier.md) does):

1. **The task commit**: the latest commit reachable from the head with `Spec-Task: <unit>/<task>` and a non-empty `Verdict:`. None: `ticked_without_evidence` — unless a note on an unreachable commit names the task (`commit_unreachable`, after a squash or rebase merge).
2. **The bundle**: every line a DSSE envelope with an in-toto Statement v1 about that commit (`malformed`, `mismatch`); no note is `missing`.
3. **Signatures** with the signer's own tool, against the **verifier's** trust — never a key the checked tree declares (`bad_signature`, `untrusted_signer`; a tool that cannot run is `tool_missing`, never a pass). A line a well-signed summary names by digest is covered by that signature.
4. **Agreement** with the commit: trailers, tree, the task line at the commit and at the head (`task.line_hash`), the requirement text (`spec.section_hash`), the summary naming exactly the other lines at the verdict's level, the policy recomputed at the parent (`mismatch`). A decision carries no verification lines.
5. **Signedness**: unsigned statements are `unsigned`, unverifiable unless the verifier's caller accepts them explicitly (and says so).
6. **Drift**: a requirement the task cites whose line at the head differs from its line in the commit's tree makes the tick `stale` — the verdict is for the earlier wording (for a trailer-only bug task: the triad digest).
7. **Reserved outcome** `regressed`: a later signed checkpoint marking an earlier task regressed fails like `unverifiable`. speccoder writes no such checkpoint; a verifier says it did not evaluate it.

**Trailer-only tasks.** A format home with no task list (Spec Kit's bug extension, `.specify/bugs/<slug>/`) is closed by a verdict commit with `Spec-Task: <unit>/bug:<slug>`; it is checked with the same rules. A verifier that cannot read such homes MUST fail a branch whose range holds one, saying so — a claim it cannot check is not proof.

**The red excerpt** is cut from the end of the captured output and signed only when clean: no absolute local path, no URL with credentials, nothing key-shaped. Otherwise `excerpt` is `null` and `withheld` true.

**Held-out publication.** `refs/speccoder/held-out/<unit>` carries exactly the held-out sets whose manifest digest a task closed on the pushed head names in its `held_out` phase (never an open task's set), each at `<key>/manifest.json` and `<key>/files/…`. Each publication commit is parented on the pushed tip and the previous publication, so the ref only grows ([where the sets are kept](@/docs/evidence.md#where-they-are-kept)).

## Example

One verified tick's task-verification predicate (abbreviated):

```json
{
  "revision": 2,
  "verdict": "verified",
  "unit": { "format": "speckit", "feature": "001-login", "dir": "specs/001-login" },
  "task": { "id": "T001", "line_hash": "3f9a…", "title": "Log in (FR-001)" },
  "tree": "9d2c…",
  "spec": { "section": "US1", "section_hash": "a41e…" },
  "trace": [
    { "requirement": "FR-001", "requirement_hash": "77b0…", "phase": "green", "result": "verified" },
    { "requirement": "US1", "scenario": "US1.1", "test": "tests/login.rs", "phase": "green", "result": "verified" }
  ],
  "integrity": { "findings": [] },
  "monitor": { "state": "clean", "findings": [] },
  "mutation": { "caught": 11, "missed": 1 },
  "strength": { "visible": { "share": 0.82 }, "combined": { "share": 0.91 }, "near_misses": [], "permitted": [] },
  "red": { "kind": "assertion", "excerpt": "assertion failed: logged_in", "excerpt_sha256": "c0de…", "output_sha256": "51f2…", "withheld": false },
  "policy_sha256": "e3b0…",
  "time": "2026-10-26T10:00:00Z"
}
```
