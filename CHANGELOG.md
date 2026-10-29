# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **Agents as executors, governed call by call.** `[executor] agent =
  "copilot"` (or `opencode`, `claude`, `codex`, `gemini`; any other ACP
  agent with `kind = "acp"` and `argv`) runs the agent over the Agent
  Client Protocol, its `npx` adapters pinned to exact versions. speccoder
  puts it into its ask-first permission mode, decides every permission
  request with the native policy and approvals (never answering
  "always"; what the agent does itself, outside the sandbox, asks you
  once — only a plain read of one plain file does not), serves its file and terminal calls through the
  protected paths, masking and OS sandbox, and judges the attempt as
  every attempt. A change the agent makes without asking (an allow rule
  in its own settings) is reported as *decides alone*.
  The report and the live card show the agent, its plan and what
  speccoder decided (asked · allowed · waited for you · refused), and say
  *decides alone* when no ask-first mode was set. A sign-in it needs stops
  the task with `EXECUTOR-LOGIN` and the agent's own login as a step.
- **Pick an agent for one run.** `speccoder run --agent opencode`, and
  the app's Run ▾ menu; `speccoder doctor --agents` and Settings ›
  Executor show each agent as installed, signed in and asks first, with
  the one step that fixes a missing tick; a failed task an agent
  attempted offers Try with the next one.

- **Every task finds its doer.** A task naming only paths in its
  feature's folder is planning work: it runs with planning rights —
  design documents there, never `spec.md`, `tasks.md`, `review.json`,
  checklists or the constitution — and ticks `regression_only` with the
  new reason `planning_work`; one that changes code runs again as code. A `[YOU]` task (Spec Kit) is never
  attempted by `speccoder run`; the run asks, or passes it by and ends
  with `TASK-YOURS`. So is a task whose attempt stopped `BLOCKED`, until
  the person answers.
- An acceptance author's command given as one string
  (`["cargo test --test acceptance_us1"]`) is that command's words.
- **No stop is a dead end.** An honest `BLOCKED` stop, or a `[YOU]`
  task, offers **I did it**, **Hint & retry** and **Skip**: one key each
  at a terminal (`d`/`h`/`s`), buttons in Needs You and the inspector,
  and `speccoder task done T` / `speccoder task hint T "…"` on the
  command line.
- **Run it here, copy what went wrong.** In the web app a fix line or
  next step that is a speccoder command performs or opens its view
  (doctor, explain, report, …) instead of being copied. The person's own
  fix (`npm install`, `git add … && git commit …`) runs from a card
  after a yes — as you, in the OS sandbox with the network on (the
  few fixes that change something outside the project, and machines
  without a sandbox, say they run unsandboxed), streamed, cancellable
  with everything it started — and only as speccoder issued it: built as
  argvs (a file name stays one argument) or a constant fix line, signed
  by the host; speccoder's own commands run as speccoder; sign-ins open a terminal
  (window) or copy (tab); `export` says why it stays in your shell.
  Every error has **Copy** (one masked block: code, message, fixes,
  docs, version · OS · run) and **Copy Bug Report**; `--copy` does the
  same at the CLI. Results carry `fix_steps`, next steps and actions a
  `how`. New code `FIX-FAILED`.
- **Who does what, before you run.** `doctor --feature`, the app's
  preflight and the start of a run say which tasks are planning work or
  yours; the board and `report` mark them. The speccoder preset's tasks
  template states both rules.

### Changed

- **`[executor] kind = "claude"` and `"codex"` are gone**: write
  `agent = "claude"` / `agent = "codex"` (they run over ACP now).
  `kind = "command"` stays.
- A task line's leading `/` (`/specs/001-x/plan.md`) names the
  repository root.
- `BLOCKED`'s next steps are `task done`, `task hint` and `task skip`.

## [0.4.0] - 2026-10-28

### Added

- **Amazon Bedrock as a model provider.** `[model] provider = "bedrock"`
  with a `region`. Credentials come from AWS — a Bedrock API key
  (`AWS_BEARER_TOKEN_BEDROCK`) or an `aws login`/SSO profile — never from
  the file. `init --provider bedrock [--region]` writes
  `global.anthropic.claude-sonnet-5-5`; `doctor` names the source.
- **A run's worktree gets its dependencies.** Before the baseline, a Node
  or uv project is installed in the worktree with the toolchain's locked,
  offline install, once per lockfile and again when an attempt changes it
  (`PREPARE-FAILED` when it cannot; `[worktree] prepare = "off"` skips it).
- **A run's worktree gets the git-ignored files its tests need.**
  `[worktree] include = [".env"]` and an existing `.worktreeinclude` copy
  them from your checkout. Secret-named values in a copied `.env`, and the
  lines of a copied key file, are masked as `[redacted:NAME]` in what the
  model, the journal and the screen receive; a commit never holds a copied
  file, and a diff that adds such a value stops the task
  (`SECRET-IN-DIFF`).

### Changed

- **Built on agentplane 0.55.0.**

### Fixed

- A Node project stopped at the baseline in a run's worktree, which has no
  `node_modules/`; the worktree is now prepared first.
- A uv project's checks ran `.venv/bin/python`, which a worktree never has;
  `init` now writes `uv run --locked --offline python -m pytest …`, and a
  check run through `uv run`, `poetry run` or `uvx` allows exactly itself.
- An empty pytest suite (exit 5) failed the baseline as a broken check; it
  now passes as *no tests yet*, as nextest's `--no-tests=warn` does.

## [0.3.0] - 2026-10-27

### Added

- **Start a project from one line of intent.** `speccoder new` (and **New
  Project** in the app) plans `git init`, the stack's own scaffold (cargo,
  uv + pytest, npm + vitest, go), the format's own init (Spec Kit with the
  speccoder preset, or OpenSpec), `speccoder init --yes` and one commit as
  you. Every command is shown, network steps name the hosts they reach, and
  each runs sandboxed. The run is bound to the plan's digest and resumable;
  `--undo` moves an untouched folder to the Trash or reverts the commit.
- **Plan through**: `speccoder spec specify "…" --through tasks` (and
  `issue start N --through tasks`) runs the format's own skills in order,
  stopping only for open questions and your approval of the spec.
  `feature similar` finds a requirement a description duplicates.
- **The composer and New Feature.** ⌘N suggests the rung — bug fix, quick
  change, feature — from the words, with its reason; the New Feature sheet
  shows the skills that will run, what they write and what they cost.
- **A welcome outside a project**: New Project, Open Project…, Clone from
  GitHub…, Try the Sample, recent projects. `speccoder sample` replays a
  recorded run strictly, without a model key.
- **A test file the runner never collected fails the verdict**
  (`TESTS-NOT-COLLECTED`), decided from the toolchain's rule (Cargo, pytest,
  vitest/jest, Go), the check's JUnit report, or a listing from the check's
  own command (or its `list` command). When speccoder cannot tell, the task
  stops, never ticked (`TESTS-COLLECTION-UNKNOWN`).
- **Every regression-only tick says why** — no story, no scenarios,
  acceptance off, no usable tests, none collected, not red first, or already
  passing (naming the task that implemented it) — in the trailer, the signed
  statement, status, report, the board, the TUI and ACP. A feature with
  mostly regression-only ticks gets one *Little Was Verified* item.
- **Tests go where the runner finds them.** speccoder detects the test
  layout and gives it to the implementer and the test authors; its own
  acceptance tests go to one file per story.
- **Plan checks before the run** (`doctor --feature`, *Before you run*):
  tests named where the runner won't collect them, a story's behaviour in
  Setup or Foundational, tasks that only write tests. `speccoder task
  acceptance-at T --story US` writes a story's acceptance tests at that task.
- **`speccoder preset add`**: a bundled Spec Kit preset whose tasks template
  plans for evidence, installed by `specify preset add`. Spec Kit's scripts
  get the installed `specify`'s Python (`SPECKIT_PYTHON_EXECUTABLE`), which
  has PyYAML; `doctor` warns when there is none.

### Changed

- **Built on agentplane 0.54.0.**
- **One binary.** `speccoder-verify` is now `speccoder attest verify
  --branch` (with `attest tour` and `attest badge`); the evidence Action
  downloads `speccoder`. The check and its payload are unchanged; exit codes
  are speccoder's (`0`, `26` failure, `15` neutral). `--require-signed` is
  on by default with `--branch`.
- **Watch a project while a run works on it.** `status`, `report
  --approvals`, `inbox`, `task show --account` and the app read a running
  journal at once instead of waiting. Only a second writer is refused, at
  once. A run killed before closing its journal leaves it for the next
  writer to repair; the listings say so.
- **OpenAI's words beside its calls are shown** (Responses API), live, in
  waiting calls and in `task show --account`.
- **`speccoder eval` stops on a refused account by the provider's error
  code**, not by words in the message. Not yet recognised: Anthropic's
  "credit balance is too low" (`invalid_request_error`).

### Fixed

- A call that could not be asked about (`--no-prompt`) said only "not a
  configured check or allowed command"; it now names the command and how to
  allow it.
- `review approve --run` and `run` on a feature without tasks said "feature
  done"; now `NO-TASKS` (exit 16) names `speccoder spec tasks`.
- On macOS, `mktemp` failed inside the sandbox, so Spec Kit's scripts could
  fail.
- A planning skill that stopped incomplete gave no reason; it now quotes the
  skill.
- Flags after a description were read as part of it; a description starting
  with `-` goes after `--`.
- Reading a project (`task show --account`, a replay, `doctor`) no longer
  creates a journal or the state directory.
- The test suite no longer writes to your real state directory.

### Upgrading

- A 0.2.0 journal still opens and its runs still list and show. A decision
  recorded under 0.2.0 does not read in this build: the attempt that waited
  on it fails once, saying so, and the next `speccoder run` asks again.
  Ticks and attempt refs in git are untouched.
- Upgrade every speccoder that works on a project together: a 0.2.0
  process holds the journal alone.

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

[Unreleased]: https://github.com/hupe1980/speccoder/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/hupe1980/speccoder/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/hupe1980/speccoder/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/hupe1980/speccoder/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/hupe1980/speccoder/compare/v0.0.1...v0.1.0
[0.0.1]: https://github.com/hupe1980/speccoder/releases/tag/v0.0.1
