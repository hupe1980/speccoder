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

opens the project in your browser. The same page runs in a desktop window with `speccoder ui --window` (a build with the `app` feature). The terminal keeps the line CLI; the page is where you look at a feature and take its next step with a click.

## What you see

- **Before you run.** What `speccoder doctor` would warn about, above everything else: a missing model key, configuration a worktree would not have because it is not committed, an unsigned attestation. Each comes with the command that fixes it, ready to copy. When a failure would stop the run, the Run button says so and steps back to "Run anyway".
- **The journey.** Spec → plan → tasks → commit → run → review, where the feature is on it, and one primary button for the next step — the same step the CLI offers after a command and the dashboard's `n`. A step whose file was never written (a task list without a spec) shows as skipped, not as the next step.
- **The board.** Tasks in lanes by verdict: verified, regression only, attested, no evidence, skipped, ready, blocked. A ready card has **Run task**; the running card glows and shows its stage. Long lanes fold after five cards.
- **A task's evidence.** Click a card: the verdict, the evidence rows (acceptance, held-out, checks, attestation, the last attempt — the same rows the terminal dashboard shows), and the diff of the commit that ticked it. **Accept**, **Skip**, **Nudge** and **Run** act on the task.
- **Spec & plan.** The feature's documents rendered from Markdown (never its raw HTML), requirement IDs marked cited or uncited.
- **Report.** The verdict bar; calls waiting for a decision, with **Approve** and **Reject**; and every requirement with the tasks that prove it. When the task list cites no requirement at all, the page says so and offers the analysis that finds the gaps.
- **The dock.** The running job: its pipeline stage by stage, the tool calls with the policy's decision, tokens and money. **Stop** ends it after the current task; the attempt stays resumable.

Every action shows the CLI command it runs, so what you click is what you could type. ⌘K (Ctrl+K) opens the palette: every action, filtered as you type; Enter runs it, or copies a command that would leave your machine (a pull request).

## When a run needs you

A call that needs a person opens as a sheet over the page: what the call is (a diff or the exact command), which task it is for, why it stopped, what the sandbox allows, the model's claim labelled as a claim, and the scopes — **Yes, once** (`y`), **for the task** (`t`), **for this run** (`r`), **No** with a reason (`n`), **Stop the task** (`s`). It is the same decision as the [inline prompt](@/docs/approvals.md#inline), recorded with your name and `via web`. Questions from planning skills are answered in the same sheet.

Looking at another window? The tab's title changes to *● Needs you*, and with **Notify me** allowed once, a desktop notification calls you back.

## Keys

| Key | Does |
|---|---|
| `n` | The next step |
| `1` `2` `3` | Board, Spec & plan, Report |
| ⌘K / Ctrl+K | The palette |
| `Esc` | Close the task |
| `y` `t` `r` `n` `s` | Decide a call (in its sheet) |

The page's address follows what you look at (`?view=report&task=T003`), so reload, Back and a bookmark land in the same place for as long as the host runs.

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
