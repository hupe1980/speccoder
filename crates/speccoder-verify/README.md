# speccoder-verify

The verifier of [speccoder](https://github.com/hupe1980/speccoder)'s evidence, for maintainers who did not write the change. It checks every ticked task (`[X]`) of the units a branch touches against the signed attestation speccoder left on the task's commit (`refs/notes/speccoder`), with **your** trust — an SSH allowed-signers file or a Sigstore identity you give it, never a key the branch declares — and fails on any tick without verifying evidence.

- **No model key, no network.** It reads git objects and spawns only `git` and the signature tools the signatures name (`ssh-keygen`, `gpg`, `cosign`). It is built on `speccoder-runtime` without its `engine` feature, so the model substrate and the HTTP client are not in the binary at all.
- **Nothing from the checked tree runs.** No build, no test, no hook; it never reads a working tree.
- **One implementation.** Each tick's outcome comes from the same code as `speccoder attest verify`, so both always agree.

```sh
speccoder-verify check --head HEAD --base origin/main \
  --allowed-signers-rev origin/main:.github/speccoder/allowed_signers
speccoder-verify tour --feature 007-config --task T012
speccoder-verify badge --svg evidence.svg --endpoint evidence.json
```

Exit status: `0` every tick verifies, `1` a tick is unverifiable or stale (or a refusal), `78` nothing to check (neutral). Every subcommand takes `--json` (payloads `branch_verify` and `evidence_tour`).

On GitHub, the `speccoder-verify` Action runs it on every pull request and publishes the `speccoder / evidence` check: see [Evidence check](https://hupe1980.github.io/speccoder/docs/github/#evidence-check). Release archives for Linux (static, musl) and macOS arm64 come with checksums and build provenance from speccoder's own release workflow.

[Guide](https://hupe1980.github.io/speccoder/docs/verifier/) · [The task-verification predicate](https://hupe1980.github.io/speccoder/docs/task-verification/) · [Repository](https://github.com/hupe1980/speccoder)

Licensed under either of Apache-2.0 or MIT, at your option.
