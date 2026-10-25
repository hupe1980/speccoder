# Security policy

## Reporting a vulnerability

Please report security issues privately through [GitHub's security advisories](https://github.com/hupe1980/speccoder/security/advisories/new), not in public issues. Include what you ran, what happened and what you expected; a minimal Spec Kit project that reproduces it helps most. You will get an acknowledgement within a few days.

## Supported versions

speccoder is pre-release (0.x). Fixes go into the latest release only.

## Security model

speccoder lets a language model change code and run commands, so it assumes the code it runs is untrusted. In short:

- **Isolation.** All work happens on `speccoder/<feature>` in its own git worktree; your checkout is never touched. speccoder pushes only through `speccoder pr`, after you confirm, and never merges.
- **Policy.** Every tool call is classified before it runs: allowed, needs your approval, or denied. The spec, check configuration, `.git`, secrets, privilege escalation and git remotes are denied, and no approval lifts a denial.
- **OS sandbox.** Every command speccoder starts runs in Seatbelt (macOS) or Landlock and seccomp (Linux): writes only to the worktree, no credentials, no network beyond loopback.
- **Secrets.** API keys come from the environment only; commands get an environment without secret-looking variables.
- **Provenance.** Everything the model reads is labelled by its real source, and the runtime refuses mutating calls shaped by sources outside a closed set.
- **Verification.** The verdict comes from your checks, spec-derived tests and a diff audit — never from the model.
- **Attestations.** Every tick leaves in-toto statements signed with your git signing key in `refs/notes/speccoder`; `speccoder attest verify` re-checks them against the commit and your own trust. The key stays out of the sandbox (`~/.ssh`, `~/.gnupg` hidden, no `SSH_AUTH_SOCK`); with the sandbox off, a command could reach it.

None of this replaces review: `verified` means the spec's examples pass and nothing regressed, not that the change is correct or safe.

The full model — what each layer enforces, what it does not protect against, and how to run speccoder safely — is at **[hupe1980.github.io/speccoder/docs/security](https://hupe1980.github.io/speccoder/docs/security/)**.

## Verifying a release

Each release on GitHub lists `SHA256SUMS` and a build-provenance attestation per archive, made by `.github/workflows/release.yml` from the tagged commit:

```sh
shasum -a 256 -c SHA256SUMS --ignore-missing
gh attestation verify speccoder-<version>-<target>.tar.gz --repo hupe1980/speccoder
```

The installer checks the SHA-256 always and the attestation with `--verify-attestation`. Archives are built without access to any secret.

## Release secrets

Publishing credentials live only in the GitHub environment `release`, which only tags `v*` can use: crates.io publishes through trusted publishing (short-lived tokens bound to the release workflow), with a scoped `CARGO_REGISTRY_TOKEN` as fallback, and the Homebrew tap is updated with a short-lived token of a GitHub App installed only on `hupe1980/homebrew-tap`, limited to writing its contents. Only the repository owner can create such tags and change the environment.

