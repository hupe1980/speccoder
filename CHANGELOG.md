# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

The first release: speccoder implements spec-driven work task by task and
ticks a task only on evidence the model cannot produce or edit.

### Added

**Quick changes**

- `speccoder do "…" [--check "…"]… [--no-run]`: a small change without the
  planning ceremony and without skipping the evidence — a story, an
  approved requirement and a task in a standing `NNN-quick-changes`
  feature, committed (exactly those files), then run like any task. Quick
  changes do not wait for each other.
- The app: a Quick Change sheet (what should change; done when …; Record,
  Record and Run, ⌘↩), from Needs You, `q`, File › Quick Change… (⇧⌘N), or
  by typing the change into the palette.

**The spec as a conversation**

- Open questions: every `[NEEDS CLARIFICATION: …]` in the feature's
  documents is a question (`Q1`, where it is, the requirement it is in);
  `speccoder review answer Q1 "…"` puts the answer in its place. Inbox
  kind `question`; a required review waits for them too.
- `speccoder review edit FR-003 "…" [--approve]`: reword a requirement
  yourself; its approval lapses, or is given to the new words.
- A wording that still asks a question cannot be approved.
- What changed since the spec's last commit, and its history — every
  version and what it added, reworded or removed — in `review` and the app.
- The app's Review: question cards with an answer field and **Ask Me
  with clarify**, **Edit** on every card (a live word diff; ⌘↩ saves and
  approves), questions shown as pills, **Commit**, a timeline.

**Sign in with GitHub, tell the issue**

- `speccoder auth login` signs in through speccoder's GitHub App
  (speccoder-app) with a short code — copied to the clipboard, GitHub's
  page opened — with only the permissions speccoder needs, in the
  repositories it is installed on; then checks it can see this repository
  and gives the install link when not. The app's **Continue with GitHub**
  shows the code large and waits. `SPECCODER_GITHUB_CLIENT_ID` names an
  Enterprise server's own app; a token still works (`--with-token`).
- A 404 on github.com says to install the app on the repository, with the
  link.
- `issue start` offers to comment on the issue that it is being specified
  (feature, spec, branch), once per feature, `--assign` to assign it to
  you; `speccoder issue comment N` says it later; **Comment** in the app,
  confirmed first.

- **A splash while the app starts**: the mark (its tick drawing itself),
  the name and a progress line, in the system's light or dark, from the
  first frame — before any script; it fades into the page once the first
  data is in. The window stays hidden until its page has drawn: no black
  frame. Reduced motion: no animation.

**Read the spec by its outline**

- Spec & Plan shows the open document's outline in a side nav that stays
  in view: every H2/H3, the section being read marked, a dot per
  requirement a section names (cited or not), each phase's progress in the
  task list. Click to go, `[` `]` to step, a reading bar, and the address
  keeps the document and section.

**The workbench: menus, a status bar, context menus**

- The window has a real menu bar in the order the HIG gives — speccoder,
  File, Edit, View, Run, Review, Window, Help — with ⌘1–⌘6 for the views,
  ⌘R to run, ⌘. to stop, ⌥⌘R to review the spec; the system's Edit menu
  makes copy and paste work in the page. A menu does what the same button
  does (tested: every command the menus send is handled).
- A status bar along the bottom, VS Code's way: the feature, branch and
  commits not merged, verdicts, review progress and stale ticks on the left;
  the run and its stage, spend, the model, the sandbox, the decisions
  waiting and the connection on the right — each opens where it leads.
- Right-click a task for its commands; ↑ ↓, Return and Esc work.
- View › Toggle Sidebar and Toggle Status Bar.
- `doctor` names the model (`model  anthropic/claude-sonnet-5-5`).

**Review the spec** (requirement by requirement)

- `speccoder review [approve|request|clear] [ID…]`: each requirement is a
  card; a decision is bound to its exact wording and names who made it, and
  lapses when the wording changes (shown word by word). Change requests go
  to the format's revising skill (Spec Kit: `clarify`) in one request.
- Recorded in `review.json` beside the spec, to commit and share. The model
  never writes it: a planning skill's write is refused, and a change made
  through a command is undone and reported.
- The report, the pull request description and Needs You (`review`,
  `revise`) carry it; **Spec & Plan › Review** in the app.
- `[review] required = true`: a run waits until every requirement is
  approved in the committed review (`REVIEW-PENDING`).

**Fixed**: the toolbar overflowed and overlapped at common window widths
(tabs were all as wide as the widest); tabs now take their own width, and
the bar gives way to icons as the window narrows.

**Needs You** (the decision inbox)

- `speccoder inbox` lists every decision waiting for you across the
  project's features, the most urgent first: calls waiting for approval,
  tests held for a ruling, suspected shortcuts, ticks the spec changed
  under, tasks whose last attempt did not pass, plans to commit, finished
  branches to hand off — each with the commands that settle it (naming the
  feature when there are several). `--feature` narrows it; `--json` payload
  `inbox`; `/inbox` in an editor.
- The app opens on **Needs You**: each row decides in place in its own
  feature (approve, reject, rule, run again, nudge, skip, accept, re-check,
  commit, open the draft PR), or opens the task with its evidence; the tab
  and the page title carry the count, the window's Dock badge too.
- An action that cannot run now says why: running a task while another's
  waiting call holds the worktree.
- Rulings on held tests run from the app (`Runnable::Rule`).
- `--feature` naming no feature says `FEATURE-UNKNOWN` ("no feature has
  that name or number"); `FEATURE-GONE` is for a stale pointer only.

**When the spec changes** (drift)

- A tick is proven against the spec its commit holds. When a requirement it
  cites is reworded, added or removed and committed, the report says so —
  `1 tick(s) proven against an earlier wording of their requirements` — with
  the line then and now; the verdict stays what the evidence showed.
- Everywhere it is read: the terminal report and dashboard, the board card
  (*Spec changed: FR-001*), the inspector (a word-by-word diff), the
  report's requirement matrix (no longer counted as proven) and the pull
  request description.
- The next step is **re-check what the spec changed**: `speccoder run
  --converge`, whose assessment is told which requirements changed since
  their ticks. speccoder never unticks a task on its own.
- From git alone, in both modes: each tick's commit against the checkout's
  committed `HEAD`, a bounded number of git processes per report. Payload:
  `tasks[].drift`, `trace[].changed`, `totals.stale` (replacing
  `intent_changed`).

**The first fifteen minutes** (from a newcomer's walkthrough)

- A missing model key says `KEY-MISSING` with the `export` line from every
  command that calls a model (it said `CONFIG` → `speccoder init`, which then
  said the file exists).
- Next steps follow the error's code: a missing key leads to `doctor`, an
  existing config to `speccoder config` (`CONFIG-EXISTS`), nothing to hand
  off to `run`; fixes about a task are not offered when there is none.
- `init` shows why a check fails on the untouched code (`No module named
  pytest`), uses a project's `.venv` interpreter, checks pytest is installed,
  adds `.speccoder/` to `.gitignore`, says when there is no spec project yet
  (with the format's own command first), and offers the first spec when
  there is no feature. `specify init --here --integration claude` is the
  documented setup (the skills speccoder runs come with it).
- `doctor`: a missing signing key is optional, not a warning; output and
  cache folders (`.speccoder`, `.venv`, `node_modules`, …) are no untracked
  configuration. GitHub checks for a remote before the login (`NO-REMOTE`).
- The report's first line shows only the counts there are; hints name
  `speccoder spec …` before the editor's slash command.

**GitHub**

- `speccoder issue` lists the repository's open issues; `speccoder issue
  start 42` specifies a feature from one (the issue's title, text and
  address become the description, so the spec keeps the link).
- Built in: GitHub's REST and GraphQL APIs over TLS, as you — nothing else
  to install, github.com and Enterprise hosts alike (the remote's).
  `speccoder auth login --with-token` keeps the token in the system keychain
  (never a file); `GH_TOKEN`/`GITHUB_TOKEN` in CI; the device flow with an
  OAuth App's client ID; `auth status`, `auth logout`; Sign In in the app.
  `doctor` shows whether a token is there. `GITHUB-AUTH`, `GITHUB-FAILED`.
- `pr` closes every issue the spec names, and updates an open pull request
  — its title and description brought up to date with the evidence —
  instead of opening a second one; nothing is pushed unless the pull
  request can be opened (a GitHub repository, someone signed in).
- In the web interface: issues with **Start Feature** in the overview; the
  pull request (draft, review, checks) in the report; **Open Draft PR** and
  **Update** after a confirmation that shows what leaves the machine.

**The web interface**

- `speccoder ui`: the feature in a browser tab, served on 127.0.0.1 from the
  binary (nothing fetched from the network). The journey with one primary
  button for the next step; what `doctor` would warn about above it, each
  with its fix; a board of tasks by verdict with **Run task**; each task's
  evidence rows and diff; the spec, plan and tasks rendered; the report with
  pending calls to approve or reject and every requirement beside the tasks
  that prove it; a palette (⌘K) with each action's CLI command; a live dock.
- A call that needs a person opens as a sheet with the same scopes and keys
  as the terminal (`y t r n s`), recorded `via web`; the tab title and an
  optional desktop notification call you back when you look elsewhere.
- An overview of every feature (rings of verdicts, journey stage,
  requirements proven); a task's inspector shows the code of its ticking
  commit or, after a failed attempt, that attempt's code (never merged), with
  line numbers; a running job floats as a live capsule that turns orange
  when it needs you. The interface follows Apple's system design: system
  colours with increased-contrast text, glass only for navigation and
  controls, symbols for every verdict, a sliding segmented control,
  Spotlight-style actions, permission wording ("Allow Once", "Don't
  Allow…"), and the OS's contrast and motion settings.
- Settings: `speccoder.toml` as typed controls (switches, choices,
  steppers, commands, lists; checks and MCP servers as tables), saved only
  when every parser accepts the result, comments kept, then committed alone;
  `speccoder config [get|set|unset|commit]` does the same from the terminal,
  with a `config` JSON payload. Spend in the report: tokens, cache share,
  attempts and money per task and for the feature. Always Allow… adds an
  exact command to the project's policy as a reviewed change — no grant
  outlives a run.
- `doctor`'s signing fix names an SSH key you have (or says to make one)
  instead of always `~/.ssh/id_ed25519.pub`.
- `speccoder ui --window`: the same page in a desktop window (Tauri), built
  in on macOS (the system's WebView), on Linux with `--features
  app`; it loads the local page and navigates nowhere else, and asks for
  attention (Dock) when a run needs you.
- Code: the tree is keyboard-navigable (the W3C tree pattern), ⌘P goes to
  a file by fuzzy match, ⌘F finds in a file (the window has no find of its
  own), line numbers link (`&line=42`), and links in a Markdown preview open
  the linked file.
- Files as a folder tree; Markdown previewed with Mermaid diagrams (in Code
  and in Spec & Plan), the source a switch away. Reading a file of a project
  nested in a larger repository works (it resolved from the repository's
  top level).
- Code: the feature's branch as commits (one per tick, with its verdict),
  files and a highlighted diff with line numbers, and every file of the
  branch to read. A setup assistant for a project without `speccoder.toml`:
  what was found, the model and its key, the proposed checks, writing them
  and running them on the untouched code, the commit, the first feature.
- The host takes a per-start key (in the address's fragment, then a header),
  refuses other hosts (DNS rebinding) and cross-site writes, and serves a
  strict content policy.
- The journey marks a step whose file was never written (a task list without
  a spec) as skipped instead of offering it as the next step.

**Spec formats**

- [GitHub Spec Kit](https://github.com/github/spec-kit) projects, tested
  against Spec Kit 0.13.4, 0.15.2, 0.16.5, 1.0.13, 1.1.3 and 1.2.3 (a weekly
  job tests the newest release): features, `tasks.md` with phases, `[P]`,
  `[USn]` and `depends on`, `SPECIFY_FEATURE_DIRECTORY`, the checklist gate,
  extension hooks, and a step type for Spec Kit's workflow engine
  (`speccoder workflow install`).
- [OpenSpec](https://github.com/Fission-AI/OpenSpec) 1.0+ projects (`spec-driven`
  schema): a change runs as a chain, its scenarios become the acceptance
  tests, deltas `openspec archive` would refuse stop the run before any model
  call, and an installed `openspec` CLI is asked for a second opinion.
  `--change` selects a change; `speccoder spec archive` archives one — OpenSpec's
  checks first, then the diff, then your yes, then one commit.
- `--format`, `SPECCODER_FORMAT` and `[spec] format`; otherwise the nearest
  project decides. Kiro projects are recognised and refused as not yet
  supported.

**Planning and inspecting**

- `speccoder spec [SKILL]` (also `speccoder spec`): the format's own planning
  skills run with speccoder's model under a planning policy, answering
  clarifying questions inline.
- `speccoder inspect`: features or changes, artifacts, tasks and what blocks
  what, with stable diagnostic codes. `speccoder init` detects a project's
  checks (Cargo/nextest, npm/pnpm/yarn/bun, pytest/ruff/mypy, Go, make) and
  writes `speccoder.toml`.

**Running, judged by evidence**

- `speccoder run`: works through ready tasks (or one, `--task`) on
  `speccoder/<feature>` in a worktree of its own; your checkout is never
  touched. Before an attempt, the spec's acceptance examples become tests that
  must fail on an assertion (or because what the task adds does not exist
  yet); afterwards your checks and those tests run again and the diff is
  audited for deleted, weakened or skipped tests. Only then is the task ticked
  and committed, with its evidence in trailers (`Verdict:`, `Red:`, `Run:`, …).
- Verdicts `verified`, `regression_only`, `failed`, `not_demonstrated`; a
  flaky check is never evidence; the `full` tier runs once at the end of a
  feature; retries with a verifier-built digest (`[budget] max_attempts`);
  `best_of` samples; `--parallel N` for `[P]` tasks; mutation evidence
  (`[mutation]`); an optional gaming monitor (`[monitor]`).
- Held-out acceptance tests (`[acceptance] held_out`, default on): a second
  author call writes tests for the same scenarios with other inputs, kept out
  of the worktree and every implementer prompt; after a visibly passing
  attempt they run over the final tree, and `verified` needs them to pass. A
  failure fails the verdict and the retry learns only the scenario ID and the
  spec's words. The commit carries `Held-Out:`; a finished feature's sets are
  published under `refs/speccoder/held-out/<feature>`; `eval` reports the
  gap and supports `expect = "reject"` (a tailoring case, `tailor-slugify`,
  is caught).
- A failed call to write the acceptance tests (the provider refused it, the
  run broke) stops the task for a person instead of letting it be ticked on
  the regression checks alone; `eval` stops the suite at the first refusal
  that concerns the account (no credits, quota, a rejected key).
- `speccoder run --in-place`: for specs you keep out of git. Works in your
  clean checkout on `speccoder/<feature>` and switches back to your branch
  when the run ends. Evidence is committed before the tick; an ignored
  `tasks.md` is ticked locally and the commit says so (`Tick: local`,
  `Task-Line:`, `Intent-Digest:`, `Mode: in-place`), and `report` checks the
  local ticks against those commits. A change to the spec, the judge's inputs
  or the checkout during the run stops it; git commands that would move the
  checkout are denied, files git ignores are invisible to the model's tools,
  and the sandbox keeps `.git` read-only and ignored secrets unreadable;
  `--parallel` is refused. A killed in-place run resumes from the journal and
  ticks what it had committed; `run --in-place --return` leaves it instead,
  and `report` and the dashboard show the mode. `task accept --in-place` and
  `converge --in-place` work the same way.
- `speccoder converge`: a finished feature assessed against its spec; gaps
  become new tasks, run with `run --converge`.
- `[executor] kind = "claude" | "codex" | "command"`: Claude Code, Codex or
  your own agent makes the attempts under the same judge.
- Providers: Anthropic, OpenAI, and any OpenAI-compatible server (Ollama,
  vLLM, llama.cpp, LM Studio, …); keys from the environment only.
- Cost (`speccoder report --cost`, live progress) is priced by agentplane's
  `Pricing`: tokens written to the cache at their own rate
  (`price_cache_write_per_mtok`, `price_cache_write_1h_per_mtok`; by default
  Anthropic's 1.25× and 2× input), no longer as plain input.

**The current feature**

- `speccoder feature` lists the features with their progress and the
  current one, and switches it (`feature 2`, `feature greet`); `--clear`.
  The current feature lives in Spec Kit's own `.specify/feature.json`, so
  speccoder, Spec Kit's scripts and editor agents agree.
- Planning skills are told the feature (`SPECIFY_FEATURE_DIRECTORY`): `spec
  plan` no longer stops because the model did not record the feature
  `spec specify` created — that feature becomes current by itself.
- With several features and none current, `run` and `spec` ask at a
  terminal; the dashboard opens on a picker (`f`); a stale pointer gets a
  "did you mean" (`FEATURE-GONE`); `report` and `doctor` say which feature
  and why.
- A feature with a spec but no task list is shown as being planned, with
  the planning skills that come next (`spec plan`, `spec tasks`), in
  `report`, bare `speccoder` and the dashboard — not as an error; a
  project with no features yet points to `spec specify`.

**First-run fixes**

- `init` commits everything it wrote — `speccoder.toml` and the nextest
  profile in `.config/nextest.toml` — not only the config; a worktree holds
  only commits, and its `test` check failed with "profile `speccoder` not
  found".
- A check that cannot run on the untouched code (it cannot start, does not
  build, or a test runner ran no test) stops the run before any model call
  (`CHECK-BROKEN`), instead of two attempts that could never pass;
  `run --allow-failing-baseline` goes on anyway. `doctor` lists untracked
  configuration a worktree will not have.

**The next step, taken**

- At a terminal, after a planning step, a task decision or the report, the
  CLI offers the next step — commit the feature's planning files (exactly
  those, `spec(<feature>): <files>`), the next planning skill, the run — as
  a card with the command it stands for, below the feature's journey
  (`✓ spec ── ✓ plan ── ● tasks ── · commit ── · run ── · review`, also in
  `report`). In the dashboard, `n` hands off to it.
- One action model in the runtime (what can be done now, why, the command,
  what blocks it) behind every interface; the report carries the journey
  and the uncommitted planning files.

**Acceptance strength**

- After every `verified` verdict the acceptance tests are tested: near-miss
  variants of the change (a test-blind model call: plausible, wrong about one
  scenario) and, with `[mutation]`, mutants judged by the acceptance command
  alone. The share they reject is recorded on the tick
  (`Acceptance-Strength:`, with held-out tests also `-Visible:`), in
  `report`, the attestation, `--json` and the dashboard; survivors are named.
- An alternative the spec permits that the tests reject stops the task
  (`TESTS-OVER-STRICT`) and holds its attempt; `task rule T --tests-right`
  records a ruling and the next run ticks it without a model call;
  `run --fresh-acceptance` starts the tests over.
- `[acceptance] min_strength` strengthens a weak set once (the author is told
  what got through; the new tests must be red before and green after), then
  stops (`ACCEPTANCE-WEAK`). `strength`, `near_miss`, `permitted`,
  `strength_timeout_secs`; `[mutation] suite` and `acceptance_argv`.
- `eval` reports the median acceptance strength and over-strict stops.
- The variant author and the gaming monitor are admitted once per input:
  judging the same change again reads their answer back. A resumed attempt
  continues at its own number instead of starting over at attempt 1.

**Safety**

- Every tool call is allowed, sent to a person, or denied before it runs. The
  spec, the checks' configuration, agent configuration, `.git` and secrets are
  out of the model's reach; these inputs are re-checked after every command.
- Every command runs in an OS sandbox: Seatbelt on macOS, Landlock and
  seccomp on Linux — writes only to the worktree, no credentials, no network
  beyond loopback unless a person allows it.
- MCP servers (`[mcp.<name>]`) run per task, their tools governed like
  speccoder's own; agentplane connects them (discover first, legacy
  fallback) and narrows their schemas without widening what a server is
  sent — a schema composed with `oneOf`/`allOf`/`not` is offered as written.

**People in the loop**

- Every decision is a record in the run's journal (agentplane human tasks):
  who made it — git's `user.name`, the same name `Accepted-By` uses — how
  that is known (`asserted` at a terminal or the CLI, `connected` from an
  editor), when, and bound to the call shown; an approval of anything else is
  refused. A person's answer enters the run labelled as theirs, not as
  speccoder's own. Calls a rule decides name the rule.
- A call that needs a person **waits** instead of discarding the attempt:
  `task approve`/`task reject` (no model needed; `--call`, `--all`,
  `--scope`) record the decision, and the next `run` continues the same
  attempt from that call — nothing finished is repeated. Waits end at
  `[approvals] wait` (default 7 days) as an expiry, never as anyone's
  decision. `report --approvals` lists what waits.

- Approvals inline at the terminal, in the dashboard, in editors over ACP
  (`speccoder acp`), or later (`report --approvals`, `task approve`,
  `task reject`); every decision records who made it and through what.
- `task accept`, `task skip`, `task nudge`, `report --cost`, `report`, `pr`
  (a draft pull request with every task's evidence, after you confirm).
- Approvals show one question with its full scope: a diff of the file (or
  the whole command, or the MCP arguments), why it stopped, what the sandbox
  allows, the model's claim, and what each answer covers. Answers: once, for
  the task, for this run (never for high-risk calls), reject with a required
  reason, or stop; a note on an approval reaches the model. The same view in
  editors (diff, reason form, the run's tasks as a plan), in `report
  --approvals` and in the dashboard; `task approve --note`.
- Every stop says what to do next (`Next:`, `data.next`); notifications and
  a terminal title from your own `~/.config/speccoder/config.toml`;
  `-q`/`-v`; `--json` on every command with documented exit codes.

**Durable and replayable**

- Every model and tool call is journaled (agentplane 0.52): a killed run
  resumes at once and without repeating finished calls, tested by killing a
  recorded attempt at each of its crash points;
  `speccoder replay` re-derives a recorded run offline. Failed attempts are
  kept under `refs/speccoder/attempts/…`.

**Commands and onboarding**

- A small daily surface, grouped in `--help`: Start (`init`, `doctor`),
  Plan (`spec`), Work (`run`, `task`), Review (`report`, `pr`, `attest`);
  `help advanced` for the rest. One-task decisions read like a sentence:
  `task approve T004`, `task reject T004 --reason …`, `task accept`,
  `task skip`, `task nudge`, `task show`. `report --live`, `--cost`,
  `--approvals`, `--markdown`. `-C PATH` and `--feature` on every command.
  Bare `speccoder` opens the dashboard at a terminal.
- `init` runs the checks on the untouched code and, at a terminal, offers
  to commit `speccoder.toml` (yes by default only when they pass); it notes
  the model key and `AGENTS.md`/`CLAUDE.md`.
- `doctor`: git, the project and its tool, the configuration, the key, the
  sandbox, the state directory, the signing key, the repository
  instructions and the look, changing nothing; `--bug-report` prints a
  redacted bundle for an issue.
- `completion bash|zsh|fish|elvish|powershell`.

**The dashboard**

- `speccoder run` at a terminal runs inside a dashboard: the task list, the
  running task's pipeline stage by stage with timings, tool calls with the
  policy's decision, tokens and money. A call that needs you opens over the
  screen with the prompt's full scope and keys (`y t r n s`, `1`–`5`, `tab`
  note, `v` view all, `esc` later); decisions are recorded `via dashboard`.
- Three screens: Run, Evidence — the verdict in words that never blur and
  the rows that established it, read from the task's attestation (acceptance
  red → green, held-out, checks, diff audit, monitor, mutation, approvals and
  who decided, instructions, signature, earlier attempts, the model's claim
  last and muted) — and Report (verdict bar, totals, what waits, uncited
  requirements, convergence). `d` shows a task commit's diff (`]`/`[` hunks,
  `n`/`p` files); `?` lists the keys; `esc` goes back and never quits.
- `q` while a run works asks: finish this task then stop, stop now (the next
  `run` resumes it), or keep watching. On leaving, the run's summary is
  printed and the exit status is the run's.
- `--plain` keeps line output; bare `speccoder` and `speccoder tui` watch a
  run in another process with the same screens.

**Repository instructions**

- `AGENTS.md` reaches every attempt and the acceptance test author: one
  file per directory from the repository root down to the directories the
  task names, the nearest last (agents.md's "closest file wins"), at most
  32 KiB (farther files cut, and said), labelled as repository text and
  ranked below the task, the spec and the checks. The run summary and the
  attestation name the files with their hashes; they are journaled, so a
  replay needs none. `[context] instructions` sets the names
  (`["AGENTS.md", "CLAUDE.md"]`) or turns it off (`[]`).

**Look and accessibility**

- Colour by meaning: each task state, error, stop and warning has its own
  style, and every state keeps its word and mark — nothing is told by colour
  alone. `--color`, then `[ui] color`, then `NO_COLOR`, `FORCE_COLOR` /
  `CLICOLOR_FORCE`, `CLICOLOR=0`, `TERM=dumb`; never in `--json` or
  `--events`; styling never changes the text.
- Themes in your own settings: `auto` (your terminal's 16 colours), `dark`,
  `light`, `high-contrast`, `daltonized`, `mono`; the dashboard follows them.
- `--ascii` (also a non-UTF-8 locale): ASCII marks and plain borders.
  Tables fit the terminal's width, measured by display width.
- `--screen-reader` (`SPECCODER_SCREEN_READER`, `[ui] screen_reader`):
  labelled plain lines, numbered choices, the report instead of the
  dashboard. The project's `speccoder.toml` cannot set any of this.

**Machine contract**

- `--events` on every command: JSON Lines as the command works —
  `session.started` (version and capabilities), tasks, stages, tool calls,
  approvals requested and decided, usage, notices — ending in a `result`
  line holding what `--json` prints; `seq` without gaps, RFC 3339 `ts`; a
  reader that goes away does not break the run.
- Result envelope v2: `code`, `fix`, `docs` and `next` beside the typed
  `data`, and `payload` naming its schema. JSON Schemas (2020-12) for the
  envelope, every payload and the event line are generated from the types
  that print them, published under `/schemas/v2/` and checked by a test;
  `speccoder schema [NAME]` prints them.
- Every failure and stop names a code — `error[CODE]`, `stopped[CODE]`,
  `warning[CODE]` — with copy-pasteable `fix:` lines and a docs link;
  `speccoder explain CODE` explains it offline; the codes page is generated
  from the catalog. Wrong arguments under `--json` or `--events` are a
  result too, not clap's text.
- `report --markdown`; in GitHub Actions `run` and `report` write the report
  to the job summary (under GitHub's 1 MiB limit) and annotate tasks that
  need a look (within GitHub's ten per kind, the rest counted). The model's
  words never appear in either.

**Attestations**

- Every tick leaves signed in-toto statements in a git note
  (`refs/notes/speccoder`): a `test-result` per phase, a
  `task-verification` with the verdict, hashes, trace and decisions, and a
  SLSA verification summary naming every other line. Signed with your git
  signing key (SSH or OpenPGP), Sigstore keyless as an option, or unsigned
  and said so; signing never undoes a tick. `task accept` and `task skip` are attested
  as decisions, naming who decided.
- `speccoder attest verify` re-checks every tick against its commit, its
  trailers, task line, spec section and tree, the judge configuration at its
  parent, and the signatures against your own trust; exit 26
  (`attestation_failed`). Fifty signed ticks verify in about a second.
  `speccoder attest sign` signs what ticks left unsigned.
- `report` shows each tick's attestation and traces the spec's requirements
  to the tasks citing them; acceptance tests name the scenarios they check.
  `pr` adds an Attestation column and pushes the notes, never forced.
  Notes written at once — parallel lanes, two runs on one repository —
  never lose each other (`git notes add` does not guard its ref; speccoder
  holds a lock).
  `scripts/attest-interop.sh` checks a bundle with `ssh-keygen` and in-toto's
  Go library.

**Evaluation**

- `speccoder eval` judges attempts with hidden oracles (precision, cheats,
  honest stops, tokens per correct case; `--repeat`, `--executor`), including
  impossible tasks an honest agent must refuse; `speccoder bench import` turns
  SWE-bench instances into cases.

**Distribution**

- Release archives for macOS and Linux (x86_64, aarch64) with SHA-256
  checksums and GitHub build-provenance attestations; a shell installer
  (`--verify-attestation`); a Homebrew tap; the crates on crates.io.
- Documentation at https://hupe1980.github.io/speccoder/.

## [0.0.1] - 2026-10-20

### Added

- Name reservation placeholder for `speccoder` on crates.io.

[Unreleased]: https://github.com/hupe1980/speccoder/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/hupe1980/speccoder/releases/tag/v0.0.1
