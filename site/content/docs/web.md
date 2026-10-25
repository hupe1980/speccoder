+++
title = "The web interface"
description = "speccoder ui: the feature, its evidence and the next step in a browser tab or a desktop window, served from your machine with nothing fetched from the network."
weight = 55

[extra]
group = "Use"
+++

```sh
speccoder ui            # the project in your browser
speccoder ui --window   # the same page in a desktop window
```

The window is built in on macOS and Windows (the system's WebView); on Linux it needs `cargo install speccoder --features app` (WebKitGTK). When a call needs you, the window's Dock icon asks for attention and its title says so; after every job the Dock badge counts the decisions waiting.

Every action shows the CLI command it runs. The terminal keeps the line CLI; the page is where you look at a feature and take its next step with a click.

## What you see

Six views, in the toolbar and on `1`–`6`:

| View | Shows |
|---|---|
| **Needs You** | The start page: every decision waiting, in every feature, most urgent first |
| **Overview** | Every feature at a glance, and the repository's open GitHub issues |
| **Board** | The feature's tasks in lanes by verdict |
| **Code** | What the feature's branch changed, and its files |
| **Spec & Plan** | The feature's documents, and the spec's review |
| **Report** | Verdicts, requirements and the tasks that prove them, the pull request, spend |

The **journey** (spec → plan → tasks → commit → run → review) sits above the feature's views, with one primary button for the next step — the same step the CLI suggests after a command; `n` takes it. A step whose file was never written shows as skipped. Above it, **Before you run** lists what `speccoder doctor` would warn about (a missing model key, uncommitted configuration, an unsigned attestation), each with its fix to copy; when a failure would stop the run, the button reads **Run Anyway**.

A project without `speccoder.toml` opens in the **setup assistant** instead: what was found (git, the spec project and tool, the sandbox), the model and whether its key is set, the proposed checks, then **Set Up** (writes the file and runs the checks on the untouched code), **Commit**, and your first feature. Each step names its `speccoder init` command.

### Needs You

<figure class="shot"><img class="shot-light" src="../../screenshots/inbox-light.webp" alt="Needs You: a call waiting for approval, an open question and requirements to review, across two features" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/inbox-dark.webp" alt="Needs You: a call waiting for approval, an open question and requirements to review, across two features" width="1440" height="900" loading="lazy" decoding="async"></figure>

- Groups: Waiting for Approval, Rule on the Tests, Suspected Shortcuts, The Spec Changed, Open Questions, Review the Spec, Change Requests, Did Not Pass, Plans to Commit, Ready to Hand Off.
- Each row decides in place, in its own feature, or **Open** shows it with its evidence. An action that cannot run now says why.
- The tab and the page title carry the count. With nothing waiting, the page offers the next step.
- **Quick Change** (or `q`) opens a sheet for a small change and when it is done ([quick changes](@/docs/running.md#quick-changes)).
- The same list is `speccoder inbox` in the terminal and `/inbox` in an editor.

### Overview

<figure class="shot"><img class="shot-light" src="../../screenshots/overview-light.webp" alt="Overview: every feature with its progress and next step" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/overview-dark.webp" alt="Overview: every feature with its progress and next step" width="1440" height="900" loading="lazy" decoding="async"></figure>

- Per feature: its ring of verdicts, where it is on the journey, how many requirements evidence proves, calls waiting. A card opens its board.
- Below: the open [GitHub issues](@/docs/github.md), each with **Start Feature**; one a spec already names shows that feature and **Comment**.

### Board

<figure class="shot"><img class="shot-light" src="../../screenshots/board-light.webp" alt="The board: tasks by verdict, the next step above them" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/board-dark.webp" alt="The board: tasks by verdict, the next step above them" width="1440" height="900" loading="lazy" decoding="async"></figure>

- Lanes: Waiting for You, Running, Ready, Blocked, Verified, Regression Only, Attested, No Evidence, Skipped. A lane folds after five cards.
- A ready card has **Run**; the running card shows its stage.
- A card whose tick was proven against an earlier wording says *Spec changed: FR-001*.
- Right-click a card for Show Evidence, its actions (Run, Nudge, Skip, Accept, as they apply), Copy Task ID and Copy the Run Command. ↑ ↓ and Return choose, Esc closes.

Click a card to open the **inspector**:

- The verdict and the evidence rows: acceptance, held-out, checks, attestation, the last attempt.
- The code: the commit that ticked the task, or the code of a failed or held attempt (kept, never merged), with old and new line numbers.
- **Accept**, **Skip**, **Nudge** and **Run**.
- **The Model's Account**: what the model said it was doing, turn by turn — a claim, not evidence.
- For a stale tick, the requirement then and now, word by word.

### Code

<figure class="shot"><img class="shot-light" src="../../screenshots/code-light.webp" alt="Code: the feature branch's commits and diff" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/code-dark.webp" alt="Code: the feature branch's commits and diff" width="1440" height="900" loading="lazy" decoding="async"></figure>

- **Changes**: every commit on the feature's branch since it forked (one per ticked task, with its verdict), every file with lines added and removed, and the highlighted diff.
- **Files**: the branch as a folder tree, changed files marked. Markdown opens as a preview (Mermaid drawn), with **Source** a click away.
- Keys: ↑ ↓ move in the tree, → ← open and close, typing jumps to a name; ⌘P (Ctrl+P) goes to a file; ⌘F (Ctrl+F) finds in the open file. Clicking a line number marks the line.
- A link in a previewed Markdown file opens the linked file here; a web address opens in your browser.

### Spec & Plan

<figure class="shot"><img class="shot-light" src="../../screenshots/review-light.webp" alt="Reviewing the spec: an open question and approved requirements" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/review-dark.webp" alt="Reviewing the spec: an open question and approved requirements" width="1440" height="900" loading="lazy" decoding="async"></figure>

- The documents rendered from Markdown (raw HTML never), Mermaid drawn, requirement IDs marked cited or uncited.
- The side nav holds the open document's outline: the section you read, a dot per requirement (green: a task cites it; orange: none does), each phase's progress in the task list. `[` and `]` step through sections.
- **Review** is the spec as a conversation: open questions answered in place; every requirement a card with **Approve**, **Edit**, **Request Changes**, **Clear**; **Approve All**, **Revise the Spec**, and **Commit** for what changed ([review the spec](@/docs/planning.md#review-the-spec)).

### Report

<figure class="shot"><img class="shot-light" src="../../screenshots/report-light.webp" alt="The report: verdicts, the waiting call and every requirement with the task that proves it" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/report-dark.webp" alt="The report: verdicts, the waiting call and every requirement with the task that proves it" width="1440" height="900" loading="lazy" decoding="async"></figure>

- The verdict bar; calls waiting, with **Approve** and **Reject**.
- Every requirement with the tasks that prove it. A tick proven against an earlier wording is marked and not counted; **Re-check Against the Spec** runs convergence ([when the spec changes](@/docs/evidence.md#when-the-spec-changes)). When no task cites a requirement, the page offers the analysis that finds the gaps.
- **Pull request**: its state on GitHub (draft, review, checks), and **Open Draft PR** or **Update** after a confirmation that shows what is pushed where, and the description ([GitHub](@/docs/github.md#its-pull-request)).
- **Spend**: tokens in and out, the cached share, attempts and — with prices set — money, per task and for the feature.

### The live capsule and the status bar

A running job floats at the bottom of the page: the task, its stage and the time; orange when the run needs you. Click it for the stages, the tool calls with the policy's decision, the model's own words (muted), tokens and money. **Stop** ends the job after the current task; the attempt stays resumable.

The **status bar** shows, left to right: the feature, its branch and unmerged commits, its verdicts, requirements approved, stale ticks; then the running task and stage, spend, the model, the sandbox, decisions waiting and the connection to the host. Every item opens where it leads.

## Settings

The gear (or `,`) opens `speccoder.toml` as a settings page: each section with what it decides, each key as a control with its default and meaning, checks and MCP servers as tables. Edits gather until **Save**, which writes the file only when every parser accepts it, keeping its comments. speccoder reads committed configuration only, so the page then offers to **Commit** `speccoder.toml` alone; the `speccoder config` commands that do the same are shown.

## When a run needs you

A call that needs a person opens as a sheet: the call (a diff or the exact command), its task, why it stopped, what the sandbox allows, and the model's claim, labelled as a claim.

| Choice | Key | Covers |
|---|---|---|
| **Allow Once** | `y` | this call |
| **Allow for Task** | `t` | this class of call, for the rest of the task |
| **Allow for Run** | `r` | this class of call, in every task of the run (never for high-risk calls) |
| **Don't Allow…** | `n` | rejects, with a reason the model is told |
| **Stop Task** | `s` | stops the task; the call waits for `speccoder task approve` |
| **Always Allow…** | `a` | an exact command: allows it and adds it to `[commands] allow` (a network call: `[sandbox] network_allow`), as a change you commit |

A decision reaches at most the run; anything longer is policy. It is the same decision as the [inline prompt](@/docs/approvals.md#inline), recorded with your name and `via web`. Questions from planning skills are answered in the same sheet.

In another window, the tab's title turns to *● Needs you*; with **Notify me** (the bell) allowed once, a desktop notification calls you back.

## The window's menus

In `speccoder ui --window`, a menu item does what the same button does; one that cannot run now says why.

| Menu | Items |
|---|---|
| **speccoder** | About, Settings… ⌘,, Services, Hide, Hide Others, Show All, Quit |
| **File** | New Feature… ⌘N, Quick Change… ⇧⌘N, Start from a GitHub Issue…, Commit the Planning Files, Close Window |
| **Edit** | Undo, Redo, Cut, Copy, Paste, Select All |
| **View** | the six views ⌘1–⌘6, Toggle Sidebar ⌥⌘S, Toggle Status Bar ⌥⌘B, Actions… ⌘K, Appearance (Automatic, Light, Dark), Enter Full Screen |
| **Run** | Run the Feature ⌘R, Run the Selected Task ⇧⌘R, Re-check Against the Spec, Show the Selected Task's Evidence ⌘I, Nudge / Skip / Accept the Selected Task…, Stop ⌘. |
| **Review** | Needs You ⇧⌘I, Review the Spec ⌥⌘R, Evidence Report, Open Draft Pull Request… |
| **Window** | Minimize, Zoom, Bring All to Front |
| **Help** | speccoder Documentation, Quickstart, The App, Error Codes, Report an Issue… (in your browser) |

## Keys

| Key | Does |
|---|---|
| `1` … `6` (⌘1 … ⌘6 in the window) | Needs You, Overview, Board, Code, Spec & Plan, Report |
| `n` | The next step |
| `q` | A quick change |
| `,` | Settings |
| ⌘K / Ctrl+K | The palette: every action, filtered as you type; Enter runs it |
| `[` `]` | Previous, next section (Spec & Plan) |
| ⌘P / Ctrl+P | Go to a file (Code) |
| ⌘F / Ctrl+F | Find in the open file (Code) |
| `Esc` | Close the task |
| `y` `t` `r` `n` `s` `a` | Decide a call (in its sheet) |

An action that leaves your machine (a push, a pull request, an issue comment) first shows what it would do and waits for your yes.

The toolbar switches light, dark or automatic. The page follows the system's increased-contrast and reduced-motion settings. Its address follows what you look at (`?view=code&mode=files&file=src/lib.rs&line=42`, `?view=documents&doc=plan&section=…`, `?task=T003`), so reload, Back and a bookmark land in the same place while the host runs.

## Safety

- The host listens on 127.0.0.1 only and holds a random key for this start.
- The key reaches the page once, in the address's fragment (which a browser never sends); the page removes it from the address bar. API requests carry it in a header, never in a URL.
- Requests naming another host are refused (DNS rebinding), and so are changes from another site's page.
- The page is served with a strict content policy: it loads from and sends to this host only, cannot be framed, and is never cached.
- The interface is built into the binary; nothing is fetched from the network. The window loads the same local page and navigates nowhere else.

Do not share the printed address: it holds the key.

## Options

| Flag | Does |
|---|---|
| `--port N` | Listen on `N` (default: a free port) |
| `--no-open` | Print the address, open nothing |
| `--window` | A desktop window instead of the browser |

`speccoder --json ui --no-open` prints one result once the host listens (`payload: ui` with `url`, `port` and `root`; schema [`ui.json`](@/docs/cli.md#json-output)) and keeps serving.

`ctrl-c` stops the host and a job running in it; the interrupted attempt is journaled and resumes with the next `speccoder run`, finished calls not repeated.
