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

The window uses the system's WebView on macOS and Windows; on Linux it needs a build with `--features app` (WebKitGTK). When a call needs you, the window asks for attention; the Dock badge counts the decisions waiting.

The page offers exactly what `speccoder actions` lists, through the same dispatcher as the terminal and an editor; every button shows its command on hover.

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

The **journey** (Spec → Plan → Tasks → Commit → Run → Review) sits above a feature's views, with one button for the next step (`n`); a step whose file was never written is struck through. **Before you run** lists what `speccoder doctor` warns about, each with its fix; when a failure would stop the run, the button reads **Run Anyway**.

A project without `speccoder.toml` opens in the **setup assistant**: what was found, the model and its key, the proposed checks, then **Set Up** (what `speccoder init` does), **Commit**, and your first feature.

### Needs You

<figure class="shot"><img class="shot-light" src="../../screenshots/inbox-light.webp" alt="Needs You: a call waiting for approval, an open question and requirements to review, across two features" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/inbox-dark.webp" alt="Needs You: a call waiting for approval, an open question and requirements to review, across two features" width="1440" height="900" loading="lazy" decoding="async"></figure>

- Groups, in order: Waiting for Approval; the bug stops (Confirm a Bug's Reproduction, Could Not Reproduce, Unchanged Does Not Hold, Reproduction Too Weak, Already Fixed); Calls Waiting Together (the [batch](@/docs/approvals.md#the-asking-budget)); Rule on the Tests; Suspected Shortcuts; Little Was Verified (most ticks regression-only, the reasons by count, each task a button); The Spec Changed; Read Two Ways; Open Questions; Review the Spec; Change Requests; Refused on Autopilot; Did Not Pass; Changes Set Aside; Plans to Commit; Ready to Hand Off.
- Each row decides in place; **Open** shows it with its evidence (a bug's row opens the [Bug sheet](#bug-sheet)). A disabled button says why on hover.
- The tab and the page title carry the count. **Quick Change** (`q`) opens a sheet for a [quick change](@/docs/running.md#quick-changes). The terminal's equivalent is `speccoder inbox`.

### Overview

<figure class="shot"><img class="shot-light" src="../../screenshots/overview-light.webp" alt="Overview: every feature with its progress and next step" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/overview-dark.webp" alt="Overview: every feature with its progress and next step" width="1440" height="900" loading="lazy" decoding="async"></figure>

Per feature: its ring of verdicts, stage, tasks done, requirements proven, spend and calls waiting; a card opens its board. Below, the open [GitHub issues](@/docs/github.md#a-feature-from-an-issue), each with **Start Feature** (and **Fix as Bug** when labelled `bug`); an issue a spec already names shows that feature and **Comment**. Not signed in, the card offers **Sign In**.

### Board

<figure class="shot"><img class="shot-light" src="../../screenshots/board-light.webp" alt="The board: tasks by verdict, the next step above them" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/board-dark.webp" alt="The board: tasks by verdict, the next step above them" width="1440" height="900" loading="lazy" decoding="async"></figure>

- Lanes: Waiting for You, Running, Ready, Blocked, Verified, Regression Only, Attested, No Evidence, Skipped. A lane folds after five cards.
- A ready card has **Run**; the running card shows its stage; a stale tick says *Spec changed: FR-001*; a task whose test file never ran says *Tests never ran*. A regression-only card says why on hover and to a screen reader. Right-click a card for its actions.
- **Before you run** lists what `speccoder doctor` would warn about, the plan checks included: each with its tasks as buttons and its fix.

Click a card to open the **inspector**:

- The verdict; for a stale tick, the requirement then and now, word by word.
- **Evidence**: a row per source (acceptance, held-out, checks, diff audit, gaming monitor, mutation, strength, approvals, attestation, attempts), read from the commit's attestation.
- **Tour**: the task's [evidence tour](@/docs/verifier.md#the-evidence-tour); disabled until a commit carries the task's evidence.
- **Changes**: the commit that ticked the task, or the code of a failed or held attempt (**Last Attempt**, never merged).
- **Changes set aside**: **Show Changes**, **Keep on the Branch**, **Discard…** ([changes set aside](@/docs/running.md#changes-set-aside)).
- **The Model's Account**: its reasoning and words per turn, a claim ([the model's account](@/docs/running.md#the-model-s-account)).
- The task's actions: **Run**, **Nudge**, **Skip**, **Accept**, as they apply.

### Code

<figure class="shot"><img class="shot-light" src="../../screenshots/code-light.webp" alt="Code: the feature branch's commits and diff" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/code-dark.webp" alt="Code: the feature branch's commits and diff" width="1440" height="900" loading="lazy" decoding="async"></figure>

- **Changes**: every commit on the feature's branch since it forked, with its verdict, the files and the diff.
- **Files**: the branch as a folder tree, changed files marked, with a filter. Markdown opens as a preview (Mermaid drawn; raw HTML shown as text), **Source** beside it.
- **Worktree on disk** adds untracked (**U**), modified (**M**) and ignored (dimmed) files. When such a file would stop a run, **Commit …** (or **Fix the Setup**) commits exactly those setup files in your checkout.
- Keys: ↑ ↓ → ← in the tree, typing jumps to a name; ⌘P (Ctrl+P) goes to a file; ⌘F (Ctrl+F) finds in it. A line number links to the line.
- On a task's diff, `+` beside a line adds a comment; **Send N Comments as a Nudge** sends them as one nudge for the task's next attempt.

### Spec & Plan

<figure class="shot"><img class="shot-light" src="../../screenshots/review-light.webp" alt="Reviewing the spec: an open question and approved requirements" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/review-dark.webp" alt="Reviewing the spec: an open question and approved requirements" width="1440" height="900" loading="lazy" decoding="async"></figure>

- The documents rendered from Markdown (raw HTML never), requirement IDs marked cited or uncited; the side nav holds the outline, a dot per requirement (green: cited; orange: not) and each phase's progress. `[` and `]` step through sections.
- **Review**: [review the spec](@/docs/planning.md#review-the-spec) in place: answer questions (**Write In**), **Approve**, **Edit**, **Request Changes**, **Clear**, **Revise the Spec**; *Since the last commit* with **Commit**.
- **Tests**: [review the tests](@/docs/planning.md#review-the-tests): **Write Tests**, then per test its scenarios, red result, **Reject…**, **Ask Again…**, **Edit**; `Held out: N` (the count only), the strength target, uncovered scenarios. A requirement read two ways shows **A Is Meant**, **B Is Meant**, **Reword…**.
- **Approve and Run** opens a sheet: what approving freezes, then **Ask on Risky Calls**, **Autopilot** (the grants it may hold, none ticked) or **Keep Refining** (keys 1–3). It says why it refuses; Autopilot is disabled without the OS sandbox. **▾** approves without a run.

#### Comments

Anchor a comment to a passage (select it, press `c`), a test line (`+`), an open question (**Add to Comments**) or a changed requirement (`+`). The **comment tray** collects them; **Send N Comments** runs one revision limited to those sections, shows each changed section word by word, and offers **Keep** and **Undo Revision** (⌘Z). Semantics: [comments](@/docs/planning.md#comments).

### Report

<figure class="shot"><img class="shot-light" src="../../screenshots/report-light.webp" alt="The report: verdicts, the waiting call and every requirement with the task that proves it" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/report-dark.webp" alt="The report: verdicts, the waiting call and every requirement with the task that proves it" width="1440" height="900" loading="lazy" decoding="async"></figure>

- The verdicts; calls waiting, with **Approve** and **Reject…**; stale ticks with **Re-check Against the Spec** ([when the spec changes](@/docs/evidence.md#when-the-spec-changes)).
- Every requirement with the tasks that prove it, and those no task cites; when tasks.md cites none, **Find the gaps** runs the analysis skill.
- **Pull Request**: its state on GitHub, and **Open Draft PR** or **Update** after a confirmation of what is pushed where ([its pull request](@/docs/github.md#its-pull-request)).
- **Spend**: tokens, the cached share, attempts and, with prices set, money.

### The live capsule and the status bar

A running job floats at the bottom of the page (task, stage, time; orange when it needs you). Click it for the stages, tool calls with their decisions, the model's words, tokens and money. **Stop** ends the job before the next task; the attempt stays resumable. The **status bar** shows the feature, branch, verdicts, approvals, stale ticks, the running task, spend, model, sandbox and decisions waiting.

A run that stops on uncommitted setup shows a card with the fix, its command and undo, and **Commit and Run Again**. A `speccoder run` from a terminal shows as a pill in the status bar (*interrupted*, with **Resume**, when that process died); a run started here meanwhile is refused.

### Bug sheet

Opened by **Report a Bug…** (⇧⌘B, `⇧B`), a bug's row in Needs You, or **Fix as Bug** on an issue. It takes the [triad](@/docs/bugs.md#the-triad) (one line per expected behaviour; **Record** needs one) and optionally the issue, your own failing test or a fix to judge; then **Record** or **Record and Reproduce**. The stage strip (Record → Reproduce → Confirm → Unchanged → Fix → Ship) follows the host's state, so a reload resumes; reproductions are confirmed with **Confirm and Fix** or **Reject with Reason**.

## Settings

The gear (`,`) opens `speccoder.toml` as a page: each key with its default and meaning, checks and MCP servers as tables. **Save** writes the file only when it parses, keeping its comments, then offers **Commit** for `speccoder.toml` alone. Prefix lists are saved separately, from their coverage preview (**Preview…**, **Save Prefix**, **Coverage**; see [prefix lists](@/docs/configuration.md#prefix-lists)).

## When a run needs you

A call that needs a person opens as a sheet with what the [inline prompt](@/docs/approvals.md#inline) shows and the same keys: **Allow Once** `y`, **Allow for Task** `t`, **Allow for Run** `r`, **Always Allow…** `a` (when offered; commit the change in Settings), **Don't Allow…** `n` (with a reason), **Stop Task** `s`. A choice not offered for this call is disabled and says why. Questions from planning skills are answered in the same sheet. The decision is recorded with your name and `via web`. In another tab the title turns to *● Needs you*; with **Notify me** allowed, a desktop notification calls you back.

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

In `speccoder ui --window`, the menus (speccoder, File, Edit, View, Run, Review, Window, Help) offer the same actions as the buttons, saying why when one cannot run now, with these shortcuts: New Feature ⌘N, Quick Change ⇧⌘N, Report a Bug ⇧⌘B, Settings ⌘,, Toggle Sidebar ⌥⌘S, Toggle Status Bar ⌥⌘B, Actions ⌘K, Run the Feature ⌘R, Run the Selected Task ⇧⌘R, Show the Selected Task's Evidence ⌘I, Stop ⌘., Needs You ⇧⌘I, Review the Spec ⌥⌘R, Write Tests ⇧⌘T, Send Comments ⇧⌘↵. **Help** opens the documentation, the quickstart, the app's page, the error codes and a new issue in your browser.

An action that leaves your machine (a push, a pull request, an issue comment) first shows what it would do and waits for your yes. The page follows the system's light or dark, increased-contrast and reduced-motion settings. The address follows what you look at (`?view=code&mode=files&file=src/lib.rs&line=42`, `?task=T003`), so reload, Back and bookmarks work while the host runs.

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

`speccoder --json ui --no-open` prints one result once the host listens (payload `ui`: `url`, `port`, `root`) and keeps serving. `ctrl-c` stops the host and any job in it; the interrupted attempt resumes with the next `speccoder run`.
