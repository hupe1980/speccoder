+++
title = "The web interface"
description = "speccoder ui: the feature, its evidence and the next step in a browser tab or a desktop window, served from your machine with nothing fetched from the network."
weight = 55

[extra]
group = "Use"
+++

```sh
speccoder ui
```

opens the project in your browser. `speccoder ui --window` opens the same page in a desktop window of its own — built in on macOS (it uses the system's WebView), and on Linux with `cargo install speccoder --features app` (WebKitGTK). The window follows the run itself: when a call needs you, its Dock icon asks for attention and its title says so. The terminal keeps the line CLI; the page is where you look at a feature and take its next step with a click.

## What you see

- **Setting up.** A project without `speccoder.toml` opens in the setup assistant: what was found (git, the spec project, the spec tool, the sandbox), the model (with whether its key is set), the checks speccoder proposes for your project, then **Set Up** — it writes the file, runs the checks on the untouched code and shows each result — and **Commit**, and finally your first feature in your own words. Each step names its `speccoder init` command.
- **Overview.** Every feature at a glance — the spec of the specs: each one's ring of verdicts, where it is on the journey, how many of its requirements are proven by evidence, and calls waiting for you. A card opens its board.
- **Before you run.** What `speccoder doctor` would warn about, above everything else: a missing model key, configuration a worktree would not have because it is not committed, an unsigned attestation. Each comes with the command that fixes it, ready to copy. When a failure would stop the run, the Run button says so and steps back to "Run anyway".
- **The journey.** Spec → plan → tasks → commit → run → review, where the feature is on it, and one primary button for the next step — the same step the CLI offers after a command and the dashboard's `n`. A step whose file was never written (a task list without a spec) shows as skipped, not as the next step.
- **The board.** Tasks in lanes by verdict: verified, regression only, attested, no evidence, skipped, ready, blocked. A ready card has **Run task**; the running card glows and shows its stage. Long lanes fold after five cards.
- **A task's evidence.** Click a card: the inspector shows the verdict, the evidence rows (acceptance, held-out, checks, attestation, the last attempt — the same rows the terminal dashboard shows), and the code: the commit that ticked the task, or — when its last attempt failed or was held — that attempt's code, kept for you to read and never merged. Files come with old and new line numbers, additions and removals counted. **Accept**, **Skip**, **Nudge** and **Run** act on the task.
- **Code.** What the feature's branch changed since it forked: every commit (one per ticked task, with its verdict), every file with lines added and removed, and the change itself — highlighted, with old and new line numbers. **Files** browses the branch as a folder tree (changed files and the folders holding them marked) and reads any file, highlighted; a Markdown file opens as its preview, Mermaid diagrams drawn, with **Source** a click away. The tree takes the keyboard (↑ ↓ to move, → ← to open and close, typing to jump to a name); ⌘P (Ctrl+P) goes to a file by a few letters of its path; ⌘F finds in the open file; clicking a line number marks the line, and the address keeps the file and the line (`?view=code&mode=files&file=src/lib.rs&line=42`). A link in a previewed Markdown file opens the linked file here, and a web address in your browser. Nothing here is the model's word: it is git's.
- **Spec & plan.** The feature's documents rendered from Markdown (never its raw HTML), Mermaid diagrams drawn, requirement IDs marked cited or uncited.
- **Report.** The verdict bar; calls waiting for a decision, with **Approve** and **Reject**; and every requirement with the tasks that prove it. When the task list cites no requirement at all, the page says so and offers the analysis that finds the gaps.
- **The live capsule.** A running job floats at the bottom of the page: the task, its stage and the time, at a glance; it turns orange when the run needs you. Click it for the pipeline stage by stage, the tool calls with the policy's decision, tokens and money. **Stop** ends the job after the current task; the attempt stays resumable.

- **Spend.** In the report: tokens in and out, the share read from the cache, attempts and — with prices set — money, per task (the dearest first) and for the feature.

## Settings

The gear (or `,`) opens **Settings**: `speccoder.toml` as System Settings shows preferences — each section with what it decides, each key as the control that fits it (a switch, a choice, a stepper, a command, a list) with its default and its meaning, and checks and MCP servers as tables you add and remove. Edits gather until **Save**, which writes the file only when every parser accepts it, keeping its comments. speccoder reads committed configuration only, so the page then shows the change and offers to **Commit** `speccoder.toml` alone. The bar shows the `speccoder config` commands that do the same.

Every action shows the CLI command it runs, so what you click is what you could type. ⌘K (Ctrl+K) opens the palette: every action, filtered as you type; Enter runs it, or copies a command that would leave your machine (a pull request).

## When a run needs you

A call that needs a person opens as a sheet over the page: what the call is (a diff or the exact command), which task it is for, why it stopped, what the sandbox allows, the model's claim labelled as a claim, and the scopes — **Allow Once** (`y`), **Allow for Task** (`t`), **Allow for Run** (`r`), **Don't Allow…** with a reason (`n`), **Stop Task** (`s`). For an exact command, **Always Allow…** (`a`) allows it now and adds it to the project's policy — `[commands] allow`, or `[sandbox] network_allow` for a network call — as a saved change you commit and review like code. There is no grant for "this session": a decision reaches at most the run, and anything longer is policy. It is the same decision as the [inline prompt](@/docs/approvals.md#inline), recorded with your name and `via web`. Questions from planning skills are answered in the same sheet.

Looking at another window? The tab's title changes to *● Needs you*, and with **Notify me** allowed once, a desktop notification calls you back.

## Keys

| Key | Does |
|---|---|
| `n` | The next step |
| `1` `2` `3` `4` `5` | Overview, Board, Code, Spec & Plan, Report |
| `,` | Settings |
| ⌘P / Ctrl+P | Go to a file (in Code) |
| ⌘F / Ctrl+F | Find in the open file (in Code) |
| ⌘K / Ctrl+K | The palette |
| `Esc` | Close the task |
| `y` `t` `r` `n` `s` `a` | Decide a call (in its sheet); `a` is Always Allow |

Light, dark or automatic is a switch in the toolbar; the page follows the system's increased-contrast and reduced-motion settings. The page's address follows what you look at (`?view=report&task=T003`), so reload, Back and a bookmark land in the same place for as long as the host runs.

## Safety

The host listens on 127.0.0.1 only and holds a random key for this start. The key reaches the page once, in the address's fragment (which a browser never sends), and the page removes it from the address bar. Every API request carries it in a header — never in a URL. Requests that name another host are refused (DNS rebinding), and so are changes from another site's page. The page is served with a strict content policy: it loads and sends to this host only, cannot be framed, and is never cached. Do not share the printed address: it holds the key.

The interface is built into the binary; nothing is fetched from the network. The desktop window loads the same local page and may navigate nowhere else.

## Options

| Flag | Does |
|---|---|
| `--port N` | Listen on `N` (default: a free port) |
| `--no-open` | Print the address, open nothing |
| `--window` | A desktop window instead of the browser (`cargo install speccoder --features app`) |

For scripts and editors, `speccoder --json ui --no-open` prints one result once the host listens (`payload: ui` with `url`, `port` and `root`; schema [`ui.json`](@/docs/cli.md#json-output)) and keeps serving.

`ctrl-c` stops the host, and a job running in it with it; the interrupted attempt is journaled and resumes with the next `speccoder run`, finished calls not repeated.
