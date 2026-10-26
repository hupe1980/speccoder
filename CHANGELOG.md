# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.2.0] - 2026-10-26

### Added

- **Bug fixes as their own lane.** `speccoder bug "<what happens>" --expected
  "<what should>"` records the bug where the format keeps them (Spec Kit's
  `bug` extension, else a standing bug unit), writes one reproduction per
  expected behaviour that must fail by an assertion, asks you once to
  confirm it, and ticks the fix only when the reproductions pass, the
  unchanged behaviour still passes, the held-out variants pass and the
  revert is rejected. `cannot_reproduce` and `already_satisfied` never tick.
  `--test` brings your own failing test, `--candidate PATCH|pr:N` judges
  someone else's fix, `--issue N` closes the issue when the fix is verified.
- **Review the oracle.** `speccoder review tests <REQ>` writes a
  requirement's acceptance tests during review: two independent authors,
  disagreements shown as ambiguities, red-checked in the sandbox, a held-out
  set sealed. `speccoder review approve --run ask|autopilot` freezes them and
  starts the run, which reuses them without another author call.
- **Anchored comments.** Comment on spec passages, tests and findings, send
  them as one revision; Undo Revision restores the documents byte for byte.
- **`speccoder-verify` and the `speccoder / evidence` check.** A small
  binary — no model, no network — checks every ticked task of a branch
  against its signed evidence with the maintainer's trust. A GitHub Action
  publishes the result as a check run on the pull request.
- **The evidence tour** of each task — requirement, frozen tests, red, diff,
  green, strength — in the check, the pull request, the app and
  `speccoder-verify tour`. The task-verification predicate is documented,
  with a JSON Schema.
- `speccoder actions [--task T] [--json]`: what can be done now, the list
  every interface offers.
- An asking budget: after `[approvals] budget` prompts (default 12) further
  calls wait together in one Needs You item (`speccoder approvals batch`).
- The model's reasoning summary (Anthropic, OpenAI Responses), shown as its
  account, never as evidence.
- `task restore` and `task drop` for changes a run set aside.
- Setup gaps a run would trip on (an uncommitted lockfile, unignored build
  output) stop it with a one-click fix (`SETUP-INCOMPLETE`, `speccoder fix`).
- The app shows runs started in a terminal, and untracked and ignored files
  in its Files view.
- `cargo binstall speccoder`.

### Changed

- **Approvals cover exactly what you saw** — the file a write reaches, a
  command's exact argv with its directory and program, an MCP tool's
  definition — are checked again when the call runs, and end with their
  run.
- **`[commands] allow` and `[sandbox] network_allow` match exact commands.**
  Prefixes go in `allow_prefix` / `network_allow_prefix` and are saved only
  after a preview of what they would allow (`speccoder config coverage`).
  Always Allow adds exactly one command. An `approvals.json` from 0.1.0 is
  refused; `speccoder doctor` says so.
- A write that removes a test is refused before it happens.
- `speccoder pr` also pushes the held-out tests of the closed tasks and adds
  each task's tour to the description.
- Decisions record the interface they were made in (`terminal`, `web`,
  `acp`).
- A feature's branch takes setup changes (configuration, `.gitignore`,
  lockfiles) from the checkout.
- `error[CHECK-BROKEN]` quotes the check's own error.
- agentplane 0.53.0.

### Fixed

- Needs You, `inbox` and `status` said nothing was waiting when another
  speccoder held the journal or the decisions could not be read; they now
  say the calls are unknown, and why.
- An OpenSpec project without a change was offered no first step.
- The app refused its own concurrent journal reads.
- `attest verify` reported a missing `cosign` as an untrusted signer.

### Security

- On a case-insensitive volume (macOS) another letter case reached a
  protected file (`Speccoder.toml`, `.CURSOR/…`); protected paths now match
  in any case.
- A write through a symlinked directory could reach a protected file;
  protection is judged on the file a write reaches.
- An approval could reach more than was shown: an Always Allow that widened
  to a prefix, a path grant a symlink carried to another file, an MCP grant
  that outlived a changed tool definition, a task grant that seeded later
  runs.
- Approve and Reject in the app, the dashboard and Needs You decided
  whichever call waited for the task; they now name the call and its
  digest, and a changed call is refused (`CALL-CHANGED`).
- A command whose arguments the model copied from content it read can be
  approved once only.

## [0.1.0] - 2026-10-25

The first release. speccoder implements GitHub Spec Kit features and
OpenSpec changes task by task, and ticks a task only on evidence the model
cannot produce or edit.

### Added

**Judged by evidence**

- Per task: baseline checks, spec-derived acceptance tests that must fail
  first (red) and pass after (green), the project's checks again, and a
  diff audit for weakened tests, skips and config edits. Verdicts:
  `verified`, `regression_only` or `failed`; a task the model cannot do
  honestly stops as `blocked`.
- Acceptance strength: plausible wrong versions of the change must fail
  the tests; a test that rejects what the spec allows goes to a person.
  Optional held-out tests, mutation testing and a gaming monitor.
- Every tick is a commit with its evidence in trailers and an in-toto
  attestation in git notes (`speccoder attest verify`).
- When the spec changes, ticks proven against the earlier wording say so,
  word by word; `run --converge` re-checks the code against the spec as it
  is and adds tasks for what no longer holds.

**Safe by construction**

- Work on `speccoder/<feature>` in its own worktree, or in place.
- An OS sandbox for every command: Seatbelt on macOS, Landlock and seccomp
  on Linux — writes only to the worktree, no credentials, no network
  beyond loopback, no Unix sockets outside the workspace.
- A policy classifies every tool call before it runs: allowed, sent to a
  person, or denied. Approvals for once, the task or the run, inline, in
  an editor (ACP), in the app or later — each recorded with who decided.
- Every model and tool call journaled by agentplane: a killed run resumes
  without repeating finished calls; any recorded run replays offline.

**The person's work**

- Needs You (`speccoder inbox`): every decision waiting in every feature,
  the most urgent first, each settled in place.
- Spec review (`speccoder review`): approve each requirement's wording or
  request changes; answer open questions in place; reword a requirement;
  see what changed and the spec's history. `[review] required` makes a
  run wait for it.
- Quick changes (`speccoder do "…"`): a story, an approved requirement and
  a task in a standing feature, committed and run like any task.
- The model's account: what the model says beside its calls, live and
  afterwards (`task show --account`), always shown as a claim.

**Interfaces**

- A terminal dashboard for `run` and bare `speccoder`; `--plain`, screen
  reader mode, ASCII and colour themes.
- The app (`speccoder ui`, `--window` for a native window):
  Needs You, Overview, Board, Code, Spec & Plan, Report and
  Settings; a menu bar, a status bar, context menus, a command palette and
  a setup assistant. Local only, on 127.0.0.1 with a per-start token.
- Editors over ACP; MCP servers as tools, pinned by digest.
- `--json` and `--events` on every command with published JSON Schemas;
  every failure carries a stable code and the command that fixes it.

**Spec formats and planning**

- Spec Kit 0.13 to 1.2: its planning skills hosted with speccoder's model
  (`speccoder spec specify|plan|tasks|…`), extension hooks, and a step
  type for Spec Kit workflows (`speccoder workflow install`).
- OpenSpec 1.0 and newer: changes, spec deltas, `openspec archive`
  preflight, `speccoder spec propose|archive`.

**Models and agents**

- Anthropic, OpenAI and any OpenAI-compatible server (Ollama, vLLM, …);
  keys from the environment only. Claude Code, Codex or any command as
  the agent making the attempts, judged the same way.
- Parallel `[P]` tasks in lanes, best-of-n attempts, retries with the
  verifier's facts, token and cost reporting.

**GitHub, without `gh`**

- `speccoder auth login` through speccoder's GitHub App (a short code in
  the browser; keychain storage); `issue start 42` specifies a feature
  from an issue and can comment on it; `pr` opens a draft whose
  description is the evidence.

**Distribution**

- Release archives for macOS (Apple silicon) and Linux (x86_64, aarch64)
  with SHA-256 checksums and build-provenance attestations; a shell installer
  (`--verify-attestation`); a Homebrew tap; the crates on crates.io.
- Documentation at https://hupe1980.github.io/speccoder/.

## [0.0.1] - 2026-10-20

### Added

- Name reservation placeholder for `speccoder` on crates.io.

[Unreleased]: https://github.com/hupe1980/speccoder/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/hupe1980/speccoder/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/hupe1980/speccoder/compare/v0.0.1...v0.1.0
[0.0.1]: https://github.com/hupe1980/speccoder/releases/tag/v0.0.1
