# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.2.0] - 2026-10-26

### Added

- **Bug fixes: `speccoder bug`.** `speccoder bug "<what happens>" --expected
  "<what should>" [--unchanged "…"]…` records the person's triad where the
  format keeps bugs — Spec Kit's first-party `bug` extension
  (`.specify/bugs/<slug>/`, written by its own assess skill and read back
  verbatim, else `BUG-ASSESSMENT-DRIFT`), else the standing unit
  `NNN-bugfixes` (Spec Kit) or `openspec/changes/bugfixes/` (OpenSpec) —
  commits exactly that as `bug: <title>`, and reproduces it: a test-blind
  author writes one test per expected behaviour, each red on the untouched
  code by an assertion twice (an absent symbol or a flaky failure is not a
  bug's red), plus sealed held-out variants. The person confirms the
  reproductions once, bound to the digest shown (terminal prompt, `speccoder
  bug confirm`, the app's Bug sheet, Needs You); they are frozen as the fix's
  acceptance tests (`PreFrozenAcceptance`, `Acceptance: bug_confirmation`).
  The fix is `verified` only when every reproduction went red → green, the
  pass-to-pass set (baseline-green tests that reference the changed code,
  and each Unchanged statement's test) passed by name, the held-out variants
  passed, the revert — the fix undone — was rejected by a reproduction
  (`BUG-NOT-DISCRIMINATING`), and a special case that only recognises the
  reproduction's input was rejected (`BUG-REPRO-WEAK`). Honest outcomes
  never tick: `cannot_reproduce` (an engine-written hand-off in Needs You)
  and `already_satisfied`. `--test PATH::NAME` brings the person's own
  failing test; `--candidate PATCH|pr:N` judges someone else's fix without
  an implementer or retries; `--issue N` adds the issue as untrusted context
  and `Fixes #N` on a verified fix. A bug in the extension's layout is a
  trailer-only task (`Spec-Task: bugs/<slug>/bug:<slug>`) that `attest verify
  --branch` and `speccoder-verify` check, stale when its triad changes;
  `speccoder workflow install --overlay bugfix` lets Spec Kit's `bugfix`
  workflow run speccoder for its test step. JSON payload `bug`, `bug.*`
  events (with public projections of the domain events), `BUG-*` codes, the
  `[bug]` settings, and six bug cases in `evals/bugs`.
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
- **Proof a maintainer can check: `speccoder-verify`.** A second, small
  binary (static on Linux, under 3 MB) checks every `[X]` of the units a
  branch touches against its signed attestation, with the maintainer's
  trust only (an allowed-signers file, or one read from git at the base
  revision, or a Sigstore identity — never a key the branch declares). No
  model key, no network, nothing from the checked tree executed: it reads
  git objects and spawns git and the signature tools. Each tick is
  `evidenced` (verified, regression only, attested), `stale` (a cited
  requirement changed since) or `unverifiable` with a reason, including
  `ticked_without_evidence`, a task line reworded after its tick and
  `commit_unreachable` after a squash merge; nothing to check is neutral,
  never success. The same check is `speccoder attest verify --branch`.
- **The `speccoder / evidence` check.** The `speccoder-verify` GitHub
  Action (`packaging/speccoder-verify-action`, to be published as
  `hupe1980/speccoder-verify`) verifies the release binary's build
  provenance and checksum, runs it on every push with the base branch's
  trust, and publishes a check run on the pull request's head: the verdict
  bar, the requirement trace, an annotation on each tick that is not
  plainly verified (on the changed requirement's line for a stale one),
  and every task's tour. Fork pull requests work from `pull_request_target`
  with nothing checked out; a read-only token falls back to the job summary
  and annotations. A neutral outcome is published as `failure` unless
  `allow-neutral: true`. An opt-in badge from the default branch.
- **The evidence tour.** Each tick as six steps — requirement, frozen
  tests, red, diff, green, strength — each labelled signed, committed or
  not recorded: in the check, in `speccoder pr`'s description, in the
  app's task inspector (Tour), and `speccoder-verify tour`.
- **The task-verification predicate, as an open specification**, with its
  JSON Schema (`schemas/task-verification-v0.json`): versioned by
  `revision` within v0, unknown fields kept and tolerated, the fields of
  later specifications reserved. New ticks sign a clean excerpt of the red
  run's output.
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

- `speccoder pr` also pushes the held-out tests of the tasks closed on the
  pushed head (`refs/speccoder/held-out/<unit>`, a fast-forward, never
  forced) — exactly the sets their signed evidence names, for features and
  standing units alike; open tasks' sets stay local. Its dry run says
  whether a workflow runs the evidence check.
- `speccoder-runtime` has a default feature `engine`; without it the crate
  is the read-only half (no agentplane, tokio, reqwest or keyring).
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

- **Approvals cover one resolved object.** "For the task", "for this run"
  and `task approve` cover the canonical file a write reaches, a command's
  exact argv with its directory and the executable it resolves to, or an
  MCP tool as its server defines it now — checked again when the call
  runs. Prompts show that object (`argv #1a2b…`, `@9f8e…`) and what changed
  since an approval of the same name. Grants end with their run; a retry
  keeps them, the next run asks again. `approvals.json` from 0.1.0 is
  refused by name (`speccoder doctor` says so).
- **`[commands] allow` and `[sandbox] network_allow` are exact.** Prefixes
  move to `[commands] allow_prefix` / `[sandbox] network_allow_prefix`,
  saved only from a preview of every journaled command they would newly
  allow (`speccoder config coverage`, Settings, `config set … --yes`); a
  bare program is refused.
- **Always Allow everywhere, and exact.** The terminal prompt, the
  dashboard, the app and editors offer it (`a`) for a command that is not
  high risk; it allows the call once and adds exactly its argv — nothing
  after it runs because of it.
- **An asking budget.** A run prompts at most `[approvals] budget` times
  (default 12); further calls wait together in one Needs You item, decided
  in one action, each call once (`speccoder approvals batch`, `B` in the
  dashboard).
- `approval.requested` and `approval.decided` name the object's class and
  short form.

### Fixed

- An OpenSpec project without a change got no first step: Setup, the
  journey and `speccoder actions` now offer `speccoder spec propose "…"`,
  as Spec Kit projects get `spec specify`.
- `attest verify` called a Sigstore signature `untrusted_signer` when
  `cosign` was not installed; a signature tool that cannot run is now
  `tool_missing` for every signer, and never passes.
- `attest verify`'s requirement check no longer runs `tar`: the commit's
  intent is written out from git objects, links and submodules left out.
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
- A decisions file (`approvals.json`) that could not be read did the same:
  the calls a lane stopped are now unknown in Needs You and `report
  --approvals`, a decision after another reports what still waits or why it
  is unknown, a run stops instead of attempting a task with the grants it
  read when it began, and an attempt's recorded approvals and rejections
  that cannot be read fail it instead of reading as none. Decisions made in
  an attempt that cannot be recorded are reported, not dropped.

### Security

- Closed four ways an approval reached more than the person saw: an
  Always Allow that widened to a prefix (`npm run build` allowed `npm run
  build --script-shell=…`), a path grant a symlinked directory carried to
  another file, an MCP grant that survived a changed tool definition, and
  a "for the task" grant that seeded every later run.
- A command whose arguments the model copied from content it read (a
  README, command output, an MCP result) names the source and can be
  approved once only — planted text never becomes a standing rule.
- A write through a symlinked directory into a protected path (the spec,
  CI configuration) is refused: protection is judged on the file reached.
- On a case-insensitive volume (macOS, Windows) another letter case reached
  a protected file (`Speccoder.toml`, `makefile`, `.CURSOR/rules.json`) and a
  new `.CLAUDE/settings.json` was not judged as agent configuration. The
  protected set, the frozen tests and the integrity snapshot now match in
  any letter case, an existing file is named as the volume stores it, and a
  write allowed on its asked name that reaches another file (a symlinked
  directory, another case) is refused by that name.
- Approve and Reject from the app, the dashboard and the inbox decided
  whatever call waited on the task when they were taken — a replacement for
  an expired call too. They now name the call shown and its digest, and a
  call that changed is refused (`CALL-CHANGED`). A batch's digest covers
  every listed call's object, and a lane's stopped call approved in a batch
  is approved once, as the batch says, not for every repeat in the run.
- A command whose program is a workspace file is bound to its content (a
  rewritten script is asked again), and a file without an execute bit is no
  longer named as the program `PATH` resolves to.
- The held-out tests `speccoder pr` publishes were written into the
  project's repository mid-feature (`refs/speccoder/held-out/<unit>`), where
  a later attempt could `git show` them. They are built in a private
  repository in the state directory and pushed from it; a ref an earlier
  version left in the project is moved there.
- The planning commit no longer takes a path that leaves the unit
  (`specs/001-x/../../src/x`).
- `speccoder-verify` and the Action: a verifier error (a git failure, a tree
  that cannot be written out, a unit that cannot be read, a head that drops
  the spec project) is an error, never `neutral` that `allow-neutral`
  passes; gpg's status lines are parsed exactly, so trust words in a
  signer's user ID are not trust; and on a push the Action takes
  `allowed_signers` from the commit before it or the default branch, never
  from the pushed commit itself.

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
