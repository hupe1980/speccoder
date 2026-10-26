+++
title = "Proof in pull requests"
description = "speccoder-verify checks every ticked task of a branch against its signed evidence, with the maintainer's trust: no model key, no network, nothing from the pull request executed. The speccoder / evidence check, the evidence tour and the badge."
weight = 96

[extra]
group = "Trust"
+++

A contributor's word that a change was tested is not proof. speccoder leaves signed statements per tick in git ([attestations](@/docs/attestations.md)); **`speccoder-verify`** is the small program a maintainer runs to check them — without installing speccoder, without a model key, and without trusting the contributor's machine. On GitHub, the [`speccoder-verify` action](@/docs/github.md#evidence-check) runs it on every push to a pull request and publishes the result as the check **`speccoder / evidence`**, which you can make required.

## What it checks

For the branch's head and the base it is compared with, the verifier takes every unit (feature, change) whose task list or intent files differ — **in full**, so a spec edit that stales an older tick is caught — and for every `[X]` there:

1. finds the latest commit reachable from the head that carries the task's verdict (`Spec-Task: <unit>/<task>` and `Verdict:`);
2. checks that commit's attestation with exactly the rules of [`speccoder attest verify`](@/docs/attestations.md#checking-them) — one implementation, so both always agree;
3. checks what only a branch shows: a tick no commit carries evidence for, a task line edited after its tick, a requirement the task cites that changed since ([drift](@/docs/evidence.md)), evidence on a commit a squash merge dropped.

| Outcome | Means | Fails the check |
|---|---|---|
| `evidenced` · `verified` | the evidence verifies: red → green on spec-derived tests | no |
| `evidenced` · `regression_only` | the evidence verifies, but nothing task-specific was shown | only with `--require-verified` |
| `evidenced` · `attested` | a person accepted it: their word, not evidence | only with `--require-verified` |
| `stale` | the evidence verifies, for an earlier wording of a requirement the task cites | yes |
| `unverifiable` | the evidence does not hold; the reason says why | yes |

Reasons for `unverifiable`: `ticked_without_evidence` (ticked by hand), `missing` (no note: push `refs/notes/speccoder`), `malformed`, `bad_signature`, `mismatch` (also a task line reworded after the tick), `untrusted_signer`, `unsigned`, `commit_unreachable` (a squash or rebase merge lost the commit), `tool_missing` (`ssh-keygen`, `gpg` or `cosign` cannot run here).

The branch's **conclusion** is `failure` when any tick fails, `success` when every tick carries verifying evidence, and `neutral` when there is nothing to check (no ticks in the touched units, or no spec format) — never success. Skipped tasks are listed, not counted. Earlier-task regressions (a later task breaking an earlier one's tests) are not evaluated by this version, and the check says so.

## Run it

Release archives: static for Linux x86-64 and arm64, and macOS arm64 — see [installation](@/docs/installation.md#the-verifier). Or `cargo install speccoder-verify --locked`.

```sh
speccoder-verify check --base origin/main \
  --allowed-signers-rev origin/main:.github/speccoder/allowed_signers
```

```text
failure: 7 verified, 1 regression only, 1 attested, 1 stale, 2 unverifiable
  007-config/T012  verified
  007-config/T013  unverifiable: ticked_without_evidence — no commit carries this task's verdict (ticked by hand?)
  007-config/T014  stale — FR-004 changed since the tick: the verdict is for the earlier wording
  …
```

**Trust comes from you, never from the branch.** Give an allowed-signers file on your machine (`--allowed-signers`), one read from git at a revision you trust (`--allowed-signers-rev <rev>:<path>`, the base branch's — a pull request that adds its own key changes nothing), or a Sigstore identity (`--certificate-identity` and `--certificate-oidc-issuer`). Without any, it refuses. OpenPGP signatures are checked against the keyring of the caller (`GNUPGHOME`). Unsigned evidence is unverifiable unless you pass `--require-signed=false`, and then the report says unsigned ticks were accepted.

**It executes nothing from the checked tree.** It reads git objects only — never a working tree — and spawns only `git` and the signature tool a signature names, with an environment holding nothing but `PATH`, `HOME`, `GNUPGHOME`, `LANG`, `TMPDIR` and `GITHUB_STEP_SUMMARY`. It opens no socket: it is built on `speccoder-runtime` without its `engine` feature, so the model substrate and the HTTP client are not in the binary at all ([layering rule 7](https://github.com/hupe1980/speccoder/blob/main/crates/speccoder/tests/layering.rs) keeps it so).

| Flag | |
|---|---|
| `--repo DIR` | the repository (default `.`) |
| `--head REV` / `--base REV` | what is checked, and against what (without a base: every unit) |
| `--all` | every unit, whatever the base |
| `--require-verified` | `regression_only` and `attested` fail too |
| `--json` | the result envelope, payload [`branch_verify`](/schemas/v2/branch_verify.json) |
| `--markdown FILE` | the check as Markdown |
| `--github-check FILE` | the check-run request bodies (`{create, updates}`), for the action |
| `--allow-neutral` | publish a neutral outcome as `neutral` (else `failure`) |
| `--github-annotations` | workflow-command annotations, and the check appended to `$GITHUB_STEP_SUMMARY` |

Exit status: `0` success, `1` failure or a refusal, `78` neutral. `speccoder attest verify --branch [--base REV]` runs the same check from speccoder itself, with your own trust (also git's `gpg.ssh.allowedSignersFile`).

## The check on the pull request

`speccoder / evidence` is created on the pull request's head commit on every push. Its title states the outcome and the counts; its summary leads with the verdict bar — `verified 7 · regression only 1 · attested 1 · stale 1 · unverifiable 2` — then the trust used and what was not evaluated; its text holds every tick, the requirement trace and each task's tour. Annotations sit on the lines they are about:

| Tick | Annotation | Where |
|---|---|---|
| unverifiable | failure, with what to do | the task line |
| regression only | warning | the task line |
| attested | notice, naming who accepted it | the task line |
| stale | warning | the changed requirement's line (else the task line) |

GitHub takes 50 annotations per request (the rest follow in updates) and 65 535 characters of summary and of text; what does not fit is counted, never silently cut — the conclusion is computed over every tick.

## The evidence tour

Each tick reads as six steps, in a fixed order, each labelled with where it comes from — **signed** (a signed statement says it), **committed** (git objects) or **not recorded**:

1. **Requirement** — the passage the task cites, as the tick's commit holds it;
2. **Frozen tests** — the acceptance tests, and where they were decided (or `not recorded`);
3. **Red** — which tests failed first, how (assertion, missing symbol), and the signed excerpt of the output;
4. **Diff** — the task commit's changes;
5. **Green** — what passed after: acceptance, held-out, the checks against their baseline;
6. **Strength** — how many wrong variants the tests reject.

A step without evidence says so in the verdict's words (`REGRESSION ONLY — nothing task-specific was shown`, `ATTESTED — a person's word, not evidence`) instead of disappearing. An unverifiable tick shows only its requirement and the reason. The tour is in the check, in `speccoder pr`'s description (collapsed), in the app's task inspector (**Tour**), and on its own:

```sh
speccoder-verify tour --feature 007-config --task T012           # Markdown
speccoder-verify tour --feature 007-config --task T012 --json    # payload evidence_tour
```

## The badge

```sh
speccoder-verify badge --allowed-signers-rev main:.github/speccoder/allowed_signers \
  --svg speccoder-evidence.svg --endpoint speccoder-evidence.json
```

The share of ticks, over every unit, whose evidence verifies and is not stale (skipped tasks excluded; `none` without ticks): green from 95 % with nothing unverifiable, yellow from 75 %, else red. The endpoint file is a [shields.io endpoint](https://shields.io/badges/endpoint-badge). The action publishes it only when you ask, only from the default branch ([badge-branch](@/docs/github.md#a-badge)).

## The predicate

What a tick signs is the [task-verification predicate](@/docs/task-verification.md), documented as an open specification with its JSON Schema, so anyone can write their own verifier.
