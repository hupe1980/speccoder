# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **A test file the runner never collected fails the verdict.** After every
  attempt, each test file it added or changed is checked against what the
  fast checks collected — the toolchain's rule (Cargo, pytest, vitest/jest,
  Go), the check's fresh JUnit report, then a listing built from that
  check's own command (or its new `list` command); ignored and skipped
  tests never count. A file nothing ran fails with the rule as the reason,
  and the retry is told where tests run (`TESTS-NOT-COLLECTED`); one
  speccoder cannot tell about stops the task, never ticked
  (`TESTS-COLLECTION-UNKNOWN`), and `task accept` then ticks the saved
  attempt's code. Acceptance tests count at green only when they collected
  what red did, what the files declare when red had no count, and at least
  one.
- **Every regression-only tick says why**: no story, no scenarios,
  acceptance off, the author's tests unusable or absent, none collected,
  not red first, or already passing — with the task whose tick implemented
  it, found by running the tests (a section's tests, for an appended
  section) on the feature's earlier ticks, or "could not be determined". In the
  `Regression-Only:` trailer, the signed statement, status, report, the
  board, the TUI and ACP; older ticks read "not recorded". A feature whose
  ticks are mostly regression-only gets one *Little Was Verified* item in
  Needs You.
- **Tests go where the runner finds them.** speccoder detects the test
  layout and gives it to the implementer and every test author; its own
  acceptance tests go to one file per story, a later task's section only
  ever appended.
- **Plan checks before the run** (`doctor --feature`, *Before you run*):
  tests named where the runner won't collect them, a story's behaviour
  planned in Setup or Foundational, tasks that only write tests — warnings
  with fixes, never a stop. `speccoder task acceptance-at T --story US`
  writes a story's acceptance tests at that task.
- **`speccoder preset add`**: a bundled Spec Kit preset whose tasks template
  plans for evidence, installed by Spec Kit's own `specify preset add` on a
  confirmed command.

### Changed

- **Built on agentplane 0.54.0.**
- **One binary.** `speccoder-verify` is folded into `speccoder attest verify
  --branch` (and `attest tour`/`attest badge`); the evidence Action downloads
  `speccoder`. The check, its trust rules and its `branch_verify` payload are
  unchanged; its exit status is speccoder's (`0` success, `26` failure, `15`
  neutral). `--require-signed` defaults to on with `--branch`
  (`--require-signed=false` accepts unsigned ticks, and says so).
- **Watch a project while a run works on it.** `speccoder status`,
  `report --approvals`, `inbox`, `task show --account` and the app read the
  journal of a run in another process at once, instead of waiting and saying
  another speccoder is working. Only a second writer — another run, or a
  decision on a waiting call — is refused while the run holds the journal,
  and at once. A read never creates, moves or writes the journal; a call past
  its deadline is not listed as waiting, `status` included. A run killed
  before it closed the journal (Ctrl-C, a closed terminal) leaves it for the
  next writer to repair: the listings say so (`unknown — the last run ended
  without closing its journal`), while the next run and every decision open
  it through their writer, which repairs it. A run's check for another
  task's waiting call, and the withdrawal of a set-aside attempt's calls,
  refuse rather than proceed when the calls cannot be read. A decision
  lists and decides through one writer, after denying the calls past their
  deadline; when `--all` or a batch fails part way, the calls decided before
  the failure are named as decided.
- **OpenAI's words beside its calls are shown.** With the Responses API, the
  model's commentary appears as what it said — live, in waiting calls and in
  `task show --account` — as Anthropic's words already did.
- **`speccoder eval` stops on a refused account by the provider's code**,
  as recorded in the journal, not by words in the message: the codes
  agentplane 0.54 classifies as the account's — OpenAI's
  `insufficient_quota`, `billing_*` and `invalid_api_key`; Anthropic's
  `authentication_error`, `billing_error` and `permission_error`; Gemini's
  and Bedrock's refused or expired credentials. A refused request no longer
  stops the suite because its message mentions "credit" or "quota". Not
  yet recognised: Anthropic's "credit balance is too low", which it sends as
  `invalid_request_error` — the suite goes on and each remaining case fails
  as refused.

### Fixed

- **Reading a project no longer creates a journal.** `task show --account`
  and a strict replay on a project speccoder never ran created an empty
  journal in the state directory, and `doctor` created the state directory
  itself; now only a run or a decision creates them. `doctor` still fails a
  state directory that cannot be made, because its path or a parent is a
  file.
- The test suite no longer writes to your real state directory
  (`~/.local/state/speccoder`). Under nextest (`just test`, CI) every test
  process runs with a fresh state directory of its own, removed when it
  ends, and git's background maintenance off; under plain `cargo test` the
  tests name throwaway directories themselves (the binaries they spawn get
  one per test process under `target/tmp`).

### Upgrading

- A project journal written by 0.2.0 still opens, and its runs still list
  and show (`report --approvals`, `inbox`, `task show --account`,
  `config coverage`). A person's decision recorded under 0.2.0 does not read
  in this build (agentplane 0.54 records a decision as a verdict): the
  attempt that waited on it fails once, saying so ("a decision recorded by
  speccoder 0.2.0 … run the task again"), and is set aside; the next
  `speccoder run` starts it afresh and asks again. `speccoder replay` of a
  run that read such a decision says the same instead of a divergence.
  Ticks and attempt refs in git are untouched; nothing needs deleting.
- While a 0.2.0 process holds a journal, a newer one reads its waiting calls
  as `unknown`.
  The other way round, a 0.2.0 run or decision waits up to 2 s for a newer
  speccoder's read, which holds the journal only while it reads; it stops
  with "another speccoder is working" only if reads keep it held longer than
  that. Upgrade every speccoder that works on a project together.

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
- With `CARGO_TARGET_DIR` set, every Rust check failed in the sandbox (it may
  not write outside the worktree); checks build in the worktree instead.
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
- Claude Code as the executor got each allowed command as a prefix rule
  (`Bash(cargo test:*)`); it gets the exact command (`Bash(cargo test)`).

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
