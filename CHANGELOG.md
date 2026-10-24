# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

The first release: speccoder implements spec-driven work task by task and
ticks a task only on evidence the model cannot produce or edit.

### Added

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
