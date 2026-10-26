+++
title = "Security and the sandbox"
description = "speccoder's security model: an isolated worktree, an OS sandbox on macOS and Linux, a policy that classifies every call, secret scrubbing, honest provenance labels — and what it does not protect against."
weight = 90

[extra]
group = "Trust"
nav = "Security and the sandbox"
+++

speccoder lets a language model change code and run commands to check it. Read this page before you point it at code you care about.

## The threat model in one sentence

**Your checks run code the model wrote.** The tests and the code under test may have been written a minute earlier by a model that read text an attacker controls: a dependency's README, an issue, a tool's output. speccoder treats the code it runs as untrusted:

| Layer | Decides | Where |
|---|---|---|
| Isolated worktree | your checkout is never touched | all work |
| Policy | *which* calls may run | every tool call, before it runs |
| OS sandbox | *what* a running command can reach | every command speccoder starts |
| Secret scrubbing | which environment a command sees | every command speccoder starts |
| Provenance labels | what may steer a mutating call | every model call |
| Verifier and audit | whether a task is done | after every attempt |

## Isolation

All work happens on the branch `speccoder/<feature>`, in a worktree under speccoder's state directory; it reaches your branch only when you merge it. speccoder never fetches or pushes on its own: `speccoder pr` pushes, after you confirm. The model writes in your checkout only through a hosted planning skill (`speccoder spec …`), which may write only the format's planning artifacts and leaves them uncommitted.

### In-place mode

[`--in-place`](@/docs/running.md#in-place-runs) runs the attempt in your checkout instead of a worktree. In its place:

- **A clean, checked checkout.** The run refuses a dirty checkout, works on `speccoder/<feature>`, and before every task checks that HEAD, the branch and the tree are as speccoder left them.
- **Frozen intent and judge.** Every spec, plan, task list, constitution, check and agent-configuration file, git-ignored ones included, is hashed at the start and compared before every task and every tick. Only the engine's own tick may change them.
- **No git that moves the checkout.** The model's `git checkout`, `switch`, `reset`, `commit`, `merge`, `rebase`, `cherry-pick`, `revert`, `stash`, `branch`, `tag`, `update-ref`, `worktree`, `am`, `apply --index` and `restore --staged` are denied.
- **Only what a worktree would hold.** The model's file tools refuse files git ignored at the start, except the protected spec, plan and tasks, which stay readable and never writable. The list is journaled, so a resume or replay decides the same way.
- **`.git` read-only, ignored secrets unreadable.** The sandbox makes the checkout's `.git` read-only and git-ignored credential-like files (`.env`, keys) unreadable. macOS: Seatbelt rules. Linux: each command starts in its own user and mount namespace with `.git` bound read-only and the secrets covered (they read as empty).
- **A guard where the sandbox cannot.** Some hosts forbid user namespaces (a container's default seccomp profile, Ubuntu's AppArmor restriction); the run says so at the start. In every case speccoder hashes `.git`'s `config`, `hooks/`, `info/` and every branch and tag but its own, and stops before the next tick when they change (`stopped[GIT-CHANGED]`). speccoder's own git calls never run hooks or an fsmonitor.

Not confined in place:

- a child process can read git-ignored files that do not look like credentials (the file tools cannot);
- without user namespaces, a command can write `.git` (the guard catches it) and read ignored secrets (nothing catches it);
- edits you make during an attempt are judged as the agent's.

A fresh clone holds the code and the evidence commits, but not an ignored `tasks.md` or its ticks.

### Who approved what

A person's decision is a journal record with their name and how speccoder knows it (`asserted` at a terminal or the CLI, `connected` from an editor over ACP), bound to the digest of the call they were shown; an approval of anything else is refused. Their answers enter the run labelled as a person's input. A decision made by a rule names the rule; an expired call names no one. See [approvals](@/docs/approvals.md#who-decided).

## The policy

Every tool call is classified before it runs.

**Denied** (no approval lifts a denial):
- writing the spec (`.specify/`, `specs/`, `openspec/`) or anything that decides verification: `speccoder.toml`, CI configuration (`.github/`, `.gitlab-ci.yml`), test-runner and build configuration (`.cargo/config*`, `.config/nextest.toml`, `pytest.ini`, `tox.ini`, jest/vitest configs, `deny.toml`, `justfile`, `Makefile`);
- writing agent configuration (`.claude/`, `.agents/`, `.codex/`, `.gemini/`, `.opencode/`, `.cursor/`, `.kiro/` and others, `.mcp.json`, `opencode.json`): hooks, settings, skills and MCP servers there run code in a later agent session, outside the sandbox;
- anything under `.git`;
- reading or writing secret-looking files (`.env*`, `*.pem`, `*.key`, SSH keys, `.netrc`, `.npmrc`, `.pypirc`);
- paths outside the worktree, symlink escapes included;
- privilege escalation (`sudo`, `su`, `doas`, `pkexec`), git remotes (`push`, `fetch`, `pull`, `clone`, `remote`, `submodule`), destructive git (`clean`, `reset --hard`, `filter-branch`), and writing git notes (attestations are speccoder's).

**Needs a person:** agent context files (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`); dependency manifests; tests that existed before the attempt; shells, `env`, `xargs`, `eval`; network tools; recursive `rm`; git commands that change state; unlisted commands; MCP tools not in `allow`; any command that asks for the network.

**Allowed:** reading the project, writing ordinary source files, the configured checks, the exact commands of `[commands] allow` and the extensions of a `[commands] allow_prefix` entry. A check that starts with options (`python3 -m pytest …`) allows exactly that command, not its bare program.

### What an approval can never become

A person's decision covers one resolved object — the canonical file a write reaches, a command's exact argv with its directory and the executable it resolves to, an MCP tool with the digest of its definition — and never outlives its run. It is resolved by a recorded step before the person is asked and checked again when the call runs, so a directory swapped for a symlink, a program shadowed on the path or a tool its server redefined is asked about again. Always Allow adds exactly one argv to `[commands] allow`; a prefix is configuration, saved only from a preview of every journaled command it would newly allow, and only while that preview is complete. Arguments the model copied from content it read (a README, command output, an MCP result) are named with their source, and such a call can be approved once only — planted text never becomes a standing rule. A run prompts at most `[approvals] budget` times; further calls wait together, so a flood of prompts cannot tire the reviewer into a yes. See [approvals](@/docs/approvals.md#what-a-decision-covers).

Three denied calls in a row end an attempt.

A test can change files no tool call touched. So after every command, speccoder re-checks the protected inputs, git-ignored ones included; a command that changed one ends the attempt with an integrity finding.

## The OS sandbox

The policy decides which commands run; the sandbox confines what they do. Every command speccoder starts for a task (checks, the model's commands, the formatter, acceptance tests, mutation tools, MCP servers) runs inside it:

- **macOS:** Seatbelt, with a profile generated per run.
- **Linux:** Landlock for the filesystem and a seccomp filter for the network, applied with `no_new_privs` by speccoder's own helper before the command starts. Needs Landlock ABI 3 (Linux 6.2 or later).

Inside, a command:

- **writes** only to the worktree, a private temporary directory and the package caches builds need (`~/.cargo/registry`, `~/.cache`, `~/.npm`, `~/go/pkg/mod`, `~/Library/Caches`, …), plus `[sandbox] writable`; not to speccoder's state, `~/.cargo/bin` or your checkout;
- **cannot read** credentials: `~/.ssh`, `~/.aws`, `~/.azure`, `~/.gnupg`, `~/.netrc`, `~/.docker`, `~/.kube`, `~/.npmrc`, `~/.pypirc`, `~/.git-credentials`, `~/.password-store`, `~/.config/gh`, `~/.config/gcloud`, `~/.cargo/credentials`, keyrings and keychains;
- **has no network** beyond loopback, so local test servers work (on Linux loopback is blocked too);
- **cannot connect to a Unix socket outside the writable paths** — the Docker daemon's, a database's, an agent's; a check that needs one lists its directory in `[sandbox] writable`;
- on macOS **cannot open applications** or send Apple events, which would run code outside the sandbox.

A command gets the network only when it is exactly in `[sandbox] network_allow`, extends a `[sandbox] network_allow_prefix` entry, or a person approves that exact call. Fetch dependencies before a run (`cargo fetch`, `npm ci`), or allow the fetching command. CI runs an escape suite against these properties on macOS and Linux.

Where the OS has no sandbox, every run says `commands run unsandboxed`; with `[sandbox] mode = "required"` it stops instead.

**Ubuntu 23.10 and later** restrict unprivileged user namespaces through AppArmor. Landlock and seccomp still apply, but without the namespace a command can write `.git` (the [git guard](#in-place-mode) catches it), read git-ignored secrets and reach daemon sockets; every run says so. To give speccoder its namespace, allow it in an AppArmor profile (the path is where your `speccoder` is installed):

```text
# /etc/apparmor.d/speccoder — then: sudo apparmor_parser -r /etc/apparmor.d/speccoder
abi <abi/4.0>,
include <tunables/global>
profile speccoder /usr/local/bin/speccoder flags=(unconfined) {
  userns,
}
```

### Tests kept from you at review

When tests are written at review, the held-out set is sealed in the state directory beside them. The app, the CLI and every event show its count, state and digest — no path, no content, no literal; the oracle's payload has no field that could hold one. `/api/file` serves project paths only, follows no symbolic link and reads an untracked file only when its real path is inside the feature's worktree; while held-out tests are overlaid on the worktree for a judge run, the Code view lists none of them and shows no untracked file; `/api/diff` shows only commits a branch, `HEAD` or a saved attempt reaches. Comments, revision turns, nudges and every author call other than the held-out author's own never carry held-out material; a test in CI holds every endpoint and event of a scripted review to that.

## Secrets

- API keys are read only from the environment, and never written to the journal or the output.
- Every command speccoder starts gets an environment without secret-looking variables: `*_API_KEY`, `*_TOKEN`, `*SECRET*`, `*PASSWORD*`, `AWS_*`, `ANTHROPIC_*`, `OPENAI_*`, `SSH_AUTH_SOCK`, `GIT_ASKPASS`, `SSH_ASKPASS`. Colour is switched off for these commands (`NO_COLOR=1`, `CARGO_TERM_COLOR=never`): speccoder reads their output, it does not show it.
- Commands run with stdin closed, a deadline for their whole process group, and capped output.

## Provenance labels

speccoder runs on the [agentplane](https://crates.io/crates/agentplane) runtime, whose gates decide by **labels**: where content came from. Everything the model reads is labelled by its real source where it enters:

- **trusted:** speccoder's own instructions, and your committed spec, plan, notes and decisions;
- **untrusted, by source:** check output quoted in a retry, worktree content (`AGENTS.md` included), MCP tool descriptions and results, speccoder's tool results, and the model's answers.

Every model call carries the labels of everything its conversation has read. A mutating call's arguments may come only from the model, speccoder's tools, the verifier, the worktree and your configured MCP tools; any other source is refused. Labels are never inferred from content, so an unanticipated source is never trusted by default.

## External agents

With `[executor] kind = "claude"`, `"codex"` or `"command"`, another agent makes the attempt with its own tools. speccoder's policy cannot refuse its individual calls, and the agent runs outside speccoder's sandbox with your environment, since it needs its own credentials; its own permission system is the boundary. The worktree, the diff audit, the checks and the verdict still apply.

## Signing keys

speccoder signs each tick's [attestations](@/docs/attestations.md) with your git signing key. What the model runs cannot reach it: the sandbox hides `~/.ssh` and `~/.gnupg`, commands get no `SSH_AUTH_SOCK`, and the policy denies writing git notes and `update-ref` on `refs/notes`. With the sandbox off, a command could read a key file or use an agent you can; `[attest] sign_at = "later"` or a touch-to-sign key keeps signing in your hands.

## Checking someone else's evidence

[`speccoder-verify`](@/docs/verifier.md) is built to run on a pull request you do not trust: it reads git objects only (no working tree), executes nothing from the checked tree (no build, test, hook or script), spawns only `git` and the signature tool a signature names with a scrubbed environment, opens no socket, and takes trust only from you — never a key the branch declares. The action takes it from a commit the checked one did not write (a pull request's base, the commit before a push to the default branch, else the default branch's tip) and fails where there is none; a check that cannot read the branch fails rather than coming out neutral. The [action](@/docs/github.md#evidence-check) downloads it only after checking its build provenance and sha256, never passes it a token, and refuses to run under `pull_request_target` when the head is checked out in the workspace: add no checkout or build step to that workflow.

## What it does not protect against

- **Code that runs during checks, within the sandbox's limits:** it can read files outside the hidden paths, use CPU and memory, and write to package caches.
- **MCP servers:** speccoder gates the calls; what a server does when called is up to the server. Configure only servers you trust.
- **What you send to the model provider:** your spec, plan, the files the model reads, command output and diffs. Do not run speccoder on code you may not share with it.
- **Prompt injection:** text in the repository or in tool output can try to steer the model. The policy, the sandbox and the verifier limit what that achieves; they do not prevent the attempt.
- **Merging unreviewed work:** `verified` is evidence, not a review.

## Running it safely

1. Keep the sandbox on (`[sandbox] mode = "required"` enforces it), and prefer a container, VM or dedicated account without your personal credentials.
2. Keep API keys in the environment and use one with a spending limit.
3. Review `speccoder.toml` after `speccoder init`, especially `[commands] allow` and `[mcp]`. Keep allowances narrow.
4. For unattended runs use `--no-prompt`: calls that need approval stop instead of waiting.
5. Read `speccoder report` and the branch's diff before merging.

To report a vulnerability, use [GitHub's private security advisories](https://github.com/hupe1980/speccoder/security/advisories/new).
