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

The window uses the system's WebView on macOS and Windows; on Linux it needs a build with `--features app` (WebKitGTK). When a call needs you, the window asks for attention and its title says so; after each job the Dock badge counts the decisions waiting.

Every button shows the command it runs on hover. The page offers exactly what `speccoder actions` lists, through the same dispatcher as the terminal and an editor, so a button and its command do the same thing.

## What you see

Six views, in the toolbar and on `1`–`6`:

| View | Shows |
|---|---|
| **Needs You** | The start page: every decision waiting, in every feature, most urgent first |
| **Overview** | Every feature at a glance, and the repository's open GitHub issues |
| **Board** | The feature's tasks in lanes by verdict |
| **Code** | What the feature's branch changed, and its files |
| **Spec & Plan** | The feature's documents, the spec's review and its tests |
| **Report** | Verdicts, requirements and the tasks that prove them, the pull request, spend |

The **journey** (Spec → Plan → Tasks → Commit → Run → Review) sits above a feature's views, with one primary button for the next step (`n`); a step whose file was never written shows struck through. **Before you run** lists what `speccoder doctor` warns about, each with its fix; when a failure would stop the run, the button reads **Run Anyway**.

A project without `speccoder.toml` opens in the **setup assistant**: what was found, the model and whether its key is set, the proposed checks, then **Set Up** (writes the file, runs the checks on the untouched code; its `speccoder init` command shown), **Commit**, and your first feature.

### Needs You

<figure class="shot"><img class="shot-light" src="../../screenshots/inbox-light.webp" alt="Needs You: a call waiting for approval, an open question and requirements to review, across two features" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/inbox-dark.webp" alt="Needs You: a call waiting for approval, an open question and requirements to review, across two features" width="1440" height="900" loading="lazy" decoding="async"></figure>

- Groups, in order: Waiting for Approval; the bug stops (Confirm a Bug's Reproduction, Could Not Reproduce, Unchanged Does Not Hold, Reproduction Too Weak, Already Fixed); Calls Waiting Together; Rule on the Tests; Suspected Shortcuts; The Spec Changed; Read Two Ways; Open Questions; Review the Spec; Change Requests; Refused on Autopilot; Did Not Pass; Changes Set Aside; Plans to Commit; Ready to Hand Off.
- **Calls Waiting Together** is the batch: once a run has used its [asking budget](@/docs/approvals.md#the-asking-budget), its further calls wait here and are approved or rejected in one decision, each call once (`speccoder approvals batch` in the terminal).
- Each row decides in place; **Open** shows it with its evidence (a bug's row opens its [Bug sheet](#bug-sheet)). A disabled button says why on hover.
- The tab and the page title carry the count; with nothing waiting, the page offers the next step.
- **Quick Change** (`q`) opens a sheet for a [quick change](@/docs/running.md#quick-changes).
- The same list is `speccoder inbox` in the terminal.

### Overview

<figure class="shot"><img class="shot-light" src="../../screenshots/overview-light.webp" alt="Overview: every feature with its progress and next step" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/overview-dark.webp" alt="Overview: every feature with its progress and next step" width="1440" height="900" loading="lazy" decoding="async"></figure>

- Per feature: its ring of verdicts, its stage, tasks done, requirements proven, spend and calls waiting. A card opens its board.
- Below: the open [GitHub issues](@/docs/github.md#a-feature-from-an-issue), each with **Start Feature** (and **Fix as Bug** when labelled `bug`); an issue a spec already names shows that feature and **Comment**. Not signed in, the card offers **Sign In** (a device code or a token).

### Board

<figure class="shot"><img class="shot-light" src="../../screenshots/board-light.webp" alt="The board: tasks by verdict, the next step above them" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/board-dark.webp" alt="The board: tasks by verdict, the next step above them" width="1440" height="900" loading="lazy" decoding="async"></figure>

- Lanes: Waiting for You, Running, Ready, Blocked, Verified, Regression Only, Attested, No Evidence, Skipped. A lane folds after five cards.
- A ready card has **Run**; the running card shows its stage; a tick proven against an earlier wording says *Spec changed: FR-001*.
- Right-click a card: Show Evidence, its actions, Copy Task ID, Copy the Run Command (↑ ↓, Return, Esc).

Click a card to open the **inspector**:

- The verdict; for a stale tick, the requirement then and now, word by word.
- **Evidence**: a row per source (acceptance, held-out, checks, diff audit, gaming monitor, mutation, strength, approvals, attestation, attempts), read from the commit's attestation.
- **Tour**: the task's [evidence tour](@/docs/verifier.md#the-evidence-tour), six steps from requirement to strength, each marked *signed*, *committed* or *not recorded*. Disabled until a commit carries the task's evidence.
- **Changes**: the commit that ticked the task, or the code of a failed or held attempt (**Last Attempt**, kept, never merged).
- **Changes set aside**, when the worktree held edits as the task's run started: **Show Changes**, **Keep on the Branch**, **Discard…** ([recovery](@/docs/recovery.md)).
- **The Model's Account**: its reasoning per turn (the conclusion first, **Show all of it**) and its words — a claim, not evidence ([the model's account](@/docs/running.md#the-model-s-account)).
- The task's actions: **Run**, **Nudge**, **Skip**, **Accept**, as they apply.

### Code

<figure class="shot"><img class="shot-light" src="../../screenshots/code-light.webp" alt="Code: the feature branch's commits and diff" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/code-dark.webp" alt="Code: the feature branch's commits and diff" width="1440" height="900" loading="lazy" decoding="async"></figure>

- **Changes**: every commit on the feature's branch since it forked (one per ticked task, with its verdict), the files with lines added and removed, and the diff.
- **Files**: the branch as a folder tree, changed files marked, with a filter. Markdown opens as a preview (Mermaid drawn; raw HTML shown as text), **Source** beside it. A link in a preview opens the linked file here; a web address opens in your browser.
- **Worktree on disk** adds what git does not show: untracked (**U**), modified (**M**) and ignored (dimmed) files. When such a file would stop a run (a lockfile never committed, build output not ignored), **Commit …** (or **Fix the Setup**) copies it into your checkout and commits exactly those setup files.
- Keys: ↑ ↓ move in the tree, → ← open and close, typing jumps to a name; ⌘P (Ctrl+P) goes to a file; ⌘F (Ctrl+F) finds in the open file. Clicking a line number links to the line.
- On a task's diff, `+` beside a line adds a comment; **Send N Comments as a Nudge** sends them as one [nudge](@/docs/running.md) for the task's next attempt. A frozen test's lines are commented on in Spec & Plan › Tests instead.

### Spec & Plan

<figure class="shot"><img class="shot-light" src="../../screenshots/review-light.webp" alt="Reviewing the spec: an open question and approved requirements" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/review-dark.webp" alt="Reviewing the spec: an open question and approved requirements" width="1440" height="900" loading="lazy" decoding="async"></figure>

- The documents rendered from Markdown (raw HTML never), Mermaid drawn, requirement IDs marked cited or uncited.
- The side nav holds the open document's outline: a dot per requirement (green: a task cites it; orange: none does) and each phase's progress in the task list. `[` and `]` step through sections.
- **Review** ([review the spec](@/docs/planning.md#review-the-spec)): open questions answered in place (**Write In**); each requirement a card with **Approve**, **Edit**, **Request Changes**, **Clear**; **Revise the Spec** for the change requests; *Since the last commit* with **Commit**.
- **Tests** ([review the tests](@/docs/planning.md#review-the-tests)): one card per requirement with scenarios. **Write Tests**, then each test with the scenarios it cites, its red result, **Reject…**, **Ask Again…** and **Edit**; `Held out: N` (the count only), the strength target and the scenarios no test covers. Two readings that disagree show as an amber strip with **A Is Meant**, **B Is Meant**, **Reword…**.
- **Approve and Run** opens a sheet: what approving freezes per requirement, and **Ask on Risky Calls**, **Autopilot** (the grants it may hold, none ticked; the sandbox line), **Keep Refining** (keys 1–3). It refuses, saying why, while a question is open, a requirement is read two ways or a run is active; Autopilot is disabled without the OS sandbox. **▾** also offers approving without a run.

#### Comments

Anchor a comment to:

- a passage: select it and press `c` (or the floating **Comment**);
- a test line: `+` beside it;
- an open question: **Add to Comments**;
- a changed requirement under *Since the last commit*: `+`.

The **comment tray** collects them by section. **Send N Comments** runs one revision limited to those sections, then shows each changed section word by word, flags changes outside your comments, and offers **Keep** and **Undo Revision** (⌘Z). A comment whose passage changed is marked *outdated* until you **Keep** or **Delete** it. Semantics: [comments](@/docs/planning.md#comments).

### Report

<figure class="shot"><img class="shot-light" src="../../screenshots/report-light.webp" alt="The report: verdicts, the waiting call and every requirement with the task that proves it" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/report-dark.webp" alt="The report: verdicts, the waiting call and every requirement with the task that proves it" width="1440" height="900" loading="lazy" decoding="async"></figure>

- The verdicts; calls waiting, with **Approve** and **Reject…**.
- Ticks proven against an earlier wording, with **Re-check Against the Spec** ([when the spec changes](@/docs/evidence.md#when-the-spec-changes)).
- Every requirement with the tasks that prove it, and the requirements no task cites. When tasks.md cites none, **Find the gaps** runs the analysis skill.
- **Pull Request**: its state on GitHub (draft, review, checks), and **Open Draft PR** or **Update** after a confirmation that shows what is pushed where ([its pull request](@/docs/github.md#its-pull-request)).
- **Spend**: tokens in and out, the cached share, attempts and — with prices set — money, per task and for the feature.

### The live capsule and the status bar

A running job floats at the bottom of the page: the task, its stage and the time; orange when the run needs you. Click it for the stages, the tool calls with the policy's decision, the model's words, tokens and money. **Stop** ends the job before the next task; the attempt stays resumable.

The **status bar** shows the feature, its branch, verdicts, approvals, stale ticks, the running task, spend, the model, the sandbox and decisions waiting; each item opens where it leads.

A run that stops on something speccoder can fix itself (setup your checkout never committed) shows a card: what stopped, what the fix changes, the command, how to undo it, and **Commit and Run Again**. Nothing is applied until you click.

A run another speccoder does for the project (a `speccoder run` in a terminal) shows as a pill in the status bar (marked *interrupted* when that process died mid-run) and on the Board's Running lane. The pill opens its task, stage, pid and spend; an interrupted run offers **Resume**. A run started here meanwhile is refused.

### Bug sheet

Opened by **File ▸ Report a Bug…** (⇧⌘B), `⇧B`, a bug's row in Needs You, or **Fix as Bug** on an issue. Fill in what happens, what should happen instead (one line per behaviour; **Record** stays disabled without one), what must keep working, and optionally the issue, your own failing test or a fix to judge; then **Record** or **Record and Reproduce**. The stage strip (Record → Reproduce → Confirm → Unchanged → Fix → Ship) follows the host's state, so a reload resumes where the bug is. Reproductions that fail on the untouched code are listed with **Confirm and Fix** and **Reject with Reason**. See [bug fixes](@/docs/bugs.md).

## Settings

The gear (`,`) opens `speccoder.toml` as a page: each section and key with its default and meaning, checks and MCP servers as tables. Edits gather until **Save**, which writes the file only when it parses, keeping its comments. speccoder reads committed configuration only, so the page then offers **Commit** for `speccoder.toml` alone.

The prefix lists (`[commands] allow_prefix`, `[sandbox] network_allow_prefix`) are not saved with the rest: **Preview…** lists every command from the journal a new prefix would newly allow, and **Save Prefix** is enabled only when that list is complete. Each existing prefix shows the same list under **Coverage** ([prefix lists](@/docs/configuration.md#prefix-lists)).

## When a run needs you

A call that needs a person opens as a sheet: the call (a diff or the exact command), the object a decision covers, its task, why it stopped, what the sandbox allows, and the model's claim, labelled as one. Questions from planning skills are answered in the same sheet.

The choices are those of the [inline prompt](@/docs/approvals.md#inline), with the same keys: **Allow Once** `y`, **Allow for Task** `t`, **Allow for Run** `r`, **Always Allow…** `a` (when offered; commit the policy change in Settings), **Don't Allow…** `n` (with a reason the model is told), **Stop Task** `s`. A choice not offered for this call (high risk, or arguments copied from content the model read) is disabled and says why. The decision is recorded with your name and `via web`.

In another tab, the title turns to *● Needs you*; with **Notify me** (the bell) allowed, a desktop notification calls you back.

## The window's menus

In `speccoder ui --window`, a menu item does what the same button does; one that cannot run now says why.

| Menu | Items |
|---|---|
| **speccoder** | About, Settings… ⌘,, Services, Hide, Hide Others, Show All, Quit |
| **File** | New Feature… ⌘N, Quick Change… ⇧⌘N, Report a Bug… ⇧⌘B, Start from a GitHub Issue…, Commit the Planning Files, Close Window |
| **Edit** | Undo, Redo, Cut, Copy, Paste, Select All |
| **View** | the six views ⌘1–⌘6, Toggle Sidebar ⌥⌘S, Toggle Status Bar ⌥⌘B, Actions… ⌘K, Appearance (Automatic, Light, Dark), Enter Full Screen |
| **Run** | Run the Feature ⌘R, Run the Selected Task ⇧⌘R, Re-check Against the Spec, Show the Selected Task's Evidence ⌘I, Nudge / Skip / Accept the Selected Task…, Stop ⌘. |
| **Review** | Needs You ⇧⌘I, Review the Spec ⌥⌘R, Evidence Report, Write Tests ⇧⌘T, Send Comments ⇧⌘↵, Undo Revision, Open Draft Pull Request… |
| **Window** | Minimize, Zoom, Bring All to Front |
| **Help** | speccoder Documentation, Quickstart, The App, Error Codes, Report an Issue… (in your browser) |

## Keys

| Key | Does |
|---|---|
| `1` … `6` (⌘1 … ⌘6 in the window) | The views |
| `n` | The next step |
| `q` / `⇧B` | A quick change / report a bug |
| `,` | Settings |
| ⌘K / Ctrl+K | The palette: every action, filtered as you type |
| ⇧⌘T / ⇧⌘↵ | Write Tests / send the comments |
| `Esc` | Close the task |

The views' own keys are listed with them above.

An action that leaves your machine (a push, a pull request, an issue comment) first shows what it would do and waits for your yes.

The toolbar switches light, dark or automatic; the page follows the system's increased-contrast and reduced-motion settings. The address follows what you look at (`?view=code&mode=files&file=src/lib.rs&line=42`, `?view=documents&doc=plan&section=…`, `?task=T003`), so reload, Back and a bookmark land in the same place while the host runs.

## Safety

- The host listens on 127.0.0.1 only, with a random key per start.
- The key reaches the page once, in the address's fragment (which a browser never sends); the page removes it from the address bar. API requests carry it in a header, never in a URL.
- Requests naming another host are refused (DNS rebinding), and so are changes from another origin.
- The page loads from and sends to this host only, cannot be framed, and is never cached.
- The interface is built into the binary; nothing is fetched from the network. The window navigates nowhere else.

Do not share the printed address: it holds the key.

## Options

| Flag | Does |
|---|---|
| `--port N` | Listen on `N` (default: a free port) |
| `--no-open` | Print the address, open nothing |
| `--window` | A desktop window instead of the browser |

`speccoder --json ui --no-open` prints one result once the host listens (payload `ui`: `url`, `port`, `root`; [JSON output](@/docs/cli.md#json-output)) and keeps serving.

`ctrl-c` stops the host and any job running in it; the interrupted attempt resumes with the next `speccoder run` ([recovery](@/docs/recovery.md)).
