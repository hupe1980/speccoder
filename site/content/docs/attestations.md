+++
title = "Attestations"
description = "The signed in-toto statements every tick leaves in git, how anyone checks them with speccoder or with ssh-keygen, gpg and cosign, and what a signature does and does not prove."
weight = 95

[extra]
group = "Trust"
+++

Anyone who can write the branch can write a commit's trailers. So every tick also leaves **signed statements**: what was checked, against which tree, under which judge configuration, with which verdict. They are signed with your git signing key and kept in git, where any reviewer re-checks them offline.

## What a tick leaves

Each ticked task commit gets a git note under `refs/notes/speccoder`. The note is an [in-toto bundle](https://github.com/in-toto/attestation/blob/main/spec/v1/bundle.md): one [DSSE envelope](https://github.com/secure-systems-lab/dsse) per line, each holding an [in-toto Statement v1](https://github.com/in-toto/attestation/blob/main/spec/v1/statement.md) about the commit (`subject: [{ "name": "<feature>/<task>", "digest": { "gitCommit": "<commit>" } }]`).

| Statement | Predicate | Says |
|---|---|---|
| One per phase | [`test-result` v0.1](https://github.com/in-toto/attestation/blob/main/spec/predicates/test-result.md) | `baseline` (the checks before the attempt), `red` (the acceptance tests failing first), `green` (tests and checks after), `held_out` (the [held-out tests](@/docs/evidence.md#held-out-tests)): which tests passed or failed, and the hash of the configuration that ran them |
| `task-verification` | `https://speccoder.dev/task-verification/v0` | The verdict, the task line's and the spec section's hashes, the tree, the requirements and scenarios it traces to, the diff audit, mutation and monitor results, sandbox, executor and model, the decisions people made during the attempt |
| The summary | [SLSA VSA v1](https://slsa.dev/spec/v1.1/verification_summary) | `verifiedLevels: ["SPECCODER_VERIFIED"]` or `["SPECCODER_REGRESSION_ONLY"]`, the judge-configuration hash as `policy`, and every other line's sha256 as `inputAttestations` |

- A task ticked with `speccoder task accept`, or passed by with `speccoder task skip`, gets one `task-verification` with verdict `attested` or `skipped` and who decided. No summary: a decision is not a verification.
- The statements never hold an absolute local path, a remote URL's credentials or anything shaped like a key; a statement that would is not written, and the run says so.
- `[attest] enabled = false` writes no statements.

## Signing

By default speccoder signs with **the key git signs your commits with**: SSH (`gpg.format = ssh` and `user.signingKey`, signed with `ssh-keygen -Y sign -n in-toto`) or OpenPGP (`gpg --detach-sign`). The `in-toto` namespace keeps a statement signature from passing as a commit signature. If you already sign commits, nothing to set up:

```sh
git config --global gpg.format ssh
git config --global user.signingKey ~/.ssh/id_ed25519.pub
```

Signing never undoes a tick: without a key, or when signing fails, the statements are stored **unsigned**, and the run, `report` and `attest verify` say so. [`[attest] signer`](@/docs/configuration.md#attest) picks the signer:

| `signer` | Signs with |
|---|---|
| `git` (default) | Your git signing key, SSH or OpenPGP |
| `sigstore` | [Sigstore](https://www.sigstore.dev/) keyless: a short-lived certificate for your identity, logged in the public transparency log. Needs `cosign` and the network. Only the summary is signed (it names every other line by digest), so a tick asks for one login |
| `none` | Nothing: the statements stay unsigned |

**Keys that need a touch** (a hardware key, a passphrase prompt) would interrupt every tick. With `sign_at = "later"` ticks store their statements unsigned, and you sign them in one go before handing off:

```sh
speccoder attest sign            # every unsigned statement of the feature's task commits (or a REV / range)
```

In CI, use a dedicated key without a passphrase or Sigstore with the CI's own identity; never give a CI job your personal key.

## Checking them

```sh
git fetch origin refs/notes/speccoder:refs/notes/speccoder   # a reviewer's clone
speccoder attest verify --feature 001-demo
```

```text
Feature    001-demo
  T002   3f1c0a9e2b  ok               ssh SHA256:Uq3… (ada@example.com)
  T003   9b7d41c0aa  mismatch         ssh SHA256:Uq3… (ada@example.com)
      bundle inputAttestations: expected 4 lines, found 3 lines
2 task object(s): 1 mismatch, 1 ok
```

Without an argument it checks every task commit on `speccoder/<feature>` and the feature's skips; `attest verify <commit>` or `attest verify main..speccoder/001-demo` picks commits. Per object it checks:

1. a note exists and every line is a statement about **this** commit;
2. every signature verifies, against **your** trust — never the project's;
3. the commit's trailers (`Spec-Task`, `Verdict`, `Run`, `Accepted-By`, `Skipped-By`) say what the statements say;
4. the task line, the spec section and the tree are those of the commit: a statement moved to another commit, or a spec rewritten after the tick, does not verify;
5. the summary names exactly the lines present — none removed, none added — at the level the verdict claims, under the judge configuration recomputed from `speccoder.toml` at the commit's parent;
6. a person's decision carries no verification statements.

| Outcome | Means |
|---|---|
| `ok` | All of the above holds and a signer you trust signed |
| `unsigned` | Everything holds, but nothing is signed. Fails with `--require-signed` |
| `untrusted_signer` | Validly signed, by a key or identity you do not trust |
| `bad_signature` | A signature does not verify: a statement was edited after signing |
| `mismatch` | A statement disagrees with the commit, its trailers or the other statements |
| `malformed` | A line is not a DSSE envelope with a statement |
| `tool_missing` | The signature's tool (`ssh-keygen`, `gpg`, `cosign`) cannot run here: nothing can be said about it, so it never passes |
| `missing` | The commit has no attestation. If another attested commit has the same tree (a rebase drops notes), it is named |

- When several apply, the worst is reported: `missing`, then `malformed`, `bad_signature`, `mismatch`, `tool_missing`, `untrusted_signer`, `unsigned`.
- Exit code 26 (`attestation_failed`): at least one object is not `ok`; `unsigned` counts only with `--require-signed`.
- `--json` gives every object with its reasons.

**Whom you trust** comes from you, not from the repository:

| Signer | Trust |
|---|---|
| SSH | git's `gpg.ssh.allowedSignersFile`, or `--allowed-signers FILE` (the [`ssh-keygen` format](https://man.openbsd.org/ssh-keygen#ALLOWED_SIGNERS): `ada@example.com namespaces="in-toto" ssh-ed25519 AAAA…`) |
| OpenPGP | Your GnuPG keyring: keys you trust fully or ultimately |
| Sigstore | `--certificate-identity` and `--certificate-oidc-issuer`. `[attest.sigstore]` in the project only *suggests* them — `speccoder pr` prints them in its verify command for you to confirm |

### On a branch

`attest verify` starts from commits. To check every `[X]` of a branch (ticks without evidence, task lines reworded after their tick, requirements changed since, evidence a squash merge lost), use [`speccoder-verify`](@/docs/verifier.md), which needs neither speccoder nor a model key, or `speccoder attest verify --branch [--base REV]` from here. Both apply the rules above. What a tick signs is specified in [the task-verification predicate](@/docs/task-verification.md).

### Without speccoder

Every line is a plain DSSE envelope, so standard tools check it. The signature covers DSSE's pre-authentication encoding, `DSSEv1 <len(type)> <type> <len(body)> <body>`:

```sh
git notes --ref=speccoder show <commit> | head -1 > line.json
type=$(jq -r .payloadType line.json)
jq -r .payload line.json | base64 -d > body.json
{ printf 'DSSEv1 %d %s %d ' ${#type} "$type" $(wc -c < body.json); cat body.json; } > pae
jq -r '.signatures[0].sig' line.json | base64 -d > line.sig
ssh-keygen -Y verify -f allowed_signers -I ada@example.com -n in-toto -s line.sig < pae
```

For OpenPGP the decoded signature is an armored detached signature (`gpg --verify line.sig pae`); for Sigstore the line's `sigstore` field is a bundle for `cosign verify-blob --bundle`. [`scripts/attest-interop.sh`](https://github.com/hupe1980/speccoder/blob/main/scripts/attest-interop.sh) does this for a whole bundle and validates every statement with in-toto's own Go library.

## In the report and the pull request

- `speccoder report` shows each tick's attestation as the note says it (`signed ssh SHA256:…`, `unsigned`, `missing`) without checking signatures, and counts ticks without one.
- `speccoder pr` adds an Attestation column to the evidence table, each task's [evidence tour](@/docs/verifier.md#the-evidence-tour) and the commands to check it, and pushes `refs/notes/speccoder` after the branch, never forced ([what it pushes](@/docs/github.md#its-pull-request)). When the remote's notes have diverged, it prints the `git notes --ref=speccoder merge` commands and opens the pull request anyway.
- Parallel lanes re-issue a task's statements for the commit that lands on the feature branch and sign them again: what is attested is the commit you merge.

## What a signature proves

A valid signature by a key you trust shows that **whoever holds that key** recorded these statements about this commit, and that nobody changed them since. With your key and signing at the tick: your machine, running speccoder, judged the commit this way.

It does not show:

- that the judging was honest if the machine was not: anyone with your key can sign anything. `~/.ssh` and `~/.gnupg` are hidden by the [sandbox](@/docs/security.md#the-os-sandbox) and commands get no `SSH_AUTH_SOCK`, but with `[sandbox] mode = "off"` a command the model runs could reach your key file or agent. Use `sign_at = "later"` or a touch-to-sign key if that matters;
- more than the evidence does ([what the evidence does not show](@/docs/evidence.md#what-the-evidence-does-not-show)): a signature makes the record tamper-evident, not the change correct.
