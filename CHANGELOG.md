# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.2.0] - 2026-10-26

### Added

- **The model's reasoning.** Each attempt turn asks the provider for a
  summary of the model's reasoning (Anthropic, OpenAI Responses;
  `[model] reasoning_summary = false` turns it off). It is shown as the
  model's account, never as evidence: `∴` lines in the terminal, a live
  thinking line in the app, ACP thoughts, the `reasoning` event, and per
  turn in `task show --account` and the inspector's timeline.
- **Changes set aside, kept or let go.** What a worktree held when a run
  started waits in Needs You and on the task's card: `speccoder task
  restore T` commits it onto the feature's branch as yours, `task drop T`
  discards it; the inspector shows the files and the diff.
- **The worktree on disk.** The app's Files view shows what git does not:
  untracked, modified and ignored paths. A lockfile never committed or build
  output not ignored — which every check run leaves behind — stops a run
  with its fix, offered there as one click (**Commit Cargo.lock**); `init`
  commits the lockfile and ignores the build output from the start.
- File-type icons in the app's file tree and diffs.
- `cargo binstall speccoder` takes the release archive.
- **`speccoder actions`.** What you can do now, by group, each with its
  command, why it is blocked and what it asks for — the list the dashboard,
  the app and an editor offer. `--task T` adds the task's own; `--json`
  prints the new `actions` payload (schema `actions.json`) for scripts,
  equal to what the app is served.
- Every action carries an `id` (its `kind` and `subject`: `accept` /
  `T004`) and a planning skill its `role` (`create`, `toward_tasks`,
  `revise`, `check`), so scripts and the app find an action by what it is,
  never by its wording; a decided call names the editor in `client`.

- **Runs in a terminal, seen in the app.** A `speccoder run` from another
  process shows in the status bar, on the Board and in Up next, with its
  stage, last call and reasoning; an interrupted one offers Resume.
- **Fixes speccoder knows, offered as one action.** A run that stops on
  setup the checkout never committed (`.config/nextest.toml`, a lockfile,
  build output not ignored) says `error[SETUP-INCOMPLETE]` and carries a
  `remedy`: its effect, the exact command and the undo. `speccoder run`
  offers it at a terminal (`[y/N]`) and runs again on a yes; the app shows
  **Commit and Run Again**; editors get a permission request; `speccoder fix
  [--dry-run]` applies it.

### Changed

- agentplane 0.53.0. In an editor (ACP) the model's words arrive as its
  message and its reasoning as its thoughts.
- Needs You counts in words (`2 tasks that did not pass`).
- A feature's branch takes setup changes from the checkout too (the
  configuration, the nextest profile, `.gitignore`, lockfiles), not only the
  spec's.
- `error[CHECK-BROKEN]` quotes the check's own error and, for a nextest
  profile never committed, the fix.
- `task drop` discards every set-aside change of the task, also those 0.1.0
  saved.
- One dispatcher: the app, the terminal's next-step and fix offers, the
  dashboard and an editor take an action the same way, with the same
  input rules and messages.
- One origin vocabulary for what a person decides through: `terminal`
  (the CLI, its prompts, the dashboard — once `cli` or `dashboard`), `web`,
  `acp` (an editor — once `editor <name>`, the name now in `client`).
  Earlier records read back.
- The app finds Setup's first step, the analysis step and every task
  action by what it is: an OpenSpec project or a reworded label changes
  nothing about what a button runs. Needs You shows an item of a kind it
  does not know under Other instead of dropping it, and its buttons say
  the action's verb (Keep, Discard, Rule).

### Fixed

- An OpenSpec project without a change got no first step: Setup, the
  journey and `speccoder actions` now offer `speccoder spec propose "…"`,
  as Spec Kit projects get `spec specify`.
- The app read the journal for several requests at once and refused
  itself ("another speccoder is working on this project"); reads now share
  one handle and wait briefly for another process.
- A write that removes a test is refused before it happens, inline tests
  in a source file included (`#[cfg(test)]` in `src/lib.rs`), and the model
  is told to keep it or report the task blocked; the audit names the
  removed tests. Before, the removal failed the attempt afterwards.
- A feature run reports the code of the task that stopped it
  (`CHECK-BROKEN`, not `CONFIG`).
- A decision on a waiting call recorded `cli` whatever interface made it
  (a reject in the app, a decision in the dashboard); it records the
  interface now.
- Calls waiting in a journal another speccoder holds were listed as
  "nothing awaits a decision". Needs You, `inbox`, `report --approvals` and
  `status` now say the calls are unknown and why (`unreadable` in the
  `approvals` payload, the `unknown` kind in `inbox`), and a decision is not
  made blind. A read no longer creates an empty journal.

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
