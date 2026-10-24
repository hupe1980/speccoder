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

**Your checks run code the model wrote.** When speccoder runs `cargo test` or `pytest` after an attempt, the tests and the code under test may have been written a minute earlier by a model that may have read text an attacker controls — a dependency's README, an issue, a tool's output. So speccoder assumes the code it runs is untrusted, and layers its defences:

| Layer | Decides | Where |
|---|---|---|
| Isolated worktree | your checkout is never touched | all work |
| Policy | *which* calls may run | every tool call, before it runs |
| OS sandbox | *what* a running command can reach | every command speccoder starts |
| Secret scrubbing | which environment a command sees | every command speccoder starts |
| Provenance labels | what may steer a mutating call | every model call |
| Verifier and audit | whether a task is done | after every attempt |

## Isolation

All work happens on the branch `speccoder/<feature>`, checked out in a worktree under speccoder's state directory. Work reaches your branch only when you merge it. speccoder never fetches and never pushes on its own; the one command that talks to a remote is `speccoder pr`, and only after you confirm. The one thing that writes in your checkout is a hosted planning skill (`speccoder skill …`), which writes Spec Kit's artifacts, uncommitted, and asks before anything else.

### In-place mode

[`--in-place`](@/docs/running.md#in-place-runs) gives up the worktree's isolation: the attempt runs in your checkout. What takes its place:

- **A clean start and a checked checkout.** The run refuses a dirty checkout, and before every task checks that HEAD, the branch and the tree are as speccoder left them; work happens on `speccoder/<feature>`, never on your branch.
- **The intent and the judge's inputs as the run found them.** Every spec, plan, task list, constitution, check and agent-configuration file — git-ignored ones included — is hashed when the run starts and compared before every task and every tick. The engine's own tick is the only change accepted.
- **No git that moves the checkout.** In place, the model's `git checkout`, `switch`, `reset`, `commit`, `merge`, `rebase`, `cherry-pick`, `revert`, `stash`, `branch`, `tag`, `update-ref`, `worktree`, `am`, `apply --index` and `restore --staged` are denied, not asked about.
- **Only what a worktree would hold.** Files git ignored when the run started are invisible to the model's file tools — `read_file`, `write_file`, `edit_file`, `list_files` and `search_code` refuse them — except the spec, plan, tasks and other protected files, which stay readable and never writable. The list is part of each attempt's journaled input, so a resumed or replayed attempt decides the same way.
- **`.git` is read-only and ignored secrets are unreadable to commands.** The OS sandbox carves the checkout's `.git` out of the writable root — a test cannot rewrite `.git/config`, plant a hook or move your branch — and makes git-ignored files that look like credentials (`.env`, keys) unreadable. On macOS these are Seatbelt rules. On Linux, Landlock cannot take a subtree back from a grant, so each command starts in a user and mount namespace of its own, with `.git` bound read-only and the secrets covered (they read as empty); the rest of the checkout stays writable.
- **A guard where the sandbox cannot.** Some hosts forbid user namespaces (a container's default seccomp profile, Ubuntu's AppArmor restriction); the run says so when it starts. Whatever the sandbox, speccoder hashes `.git`'s `config`, `hooks/`, `info/` and every branch and tag but its own when the run starts, and stops the run before the next tick when they change (`stopped[GIT-CHANGED]`). speccoder's own git calls never run hooks or an fsmonitor.

Not confined: a child process can still read git-ignored files that do not look like credentials (the file tools cannot); without user namespaces, a command can write `.git` and read ignored secrets — the guard catches the first, not the second; and edits you make during an attempt are judged as the agent's. A fresh clone holds the code and the evidence commits, but not an ignored `tasks.md` or its ticks.

### Who approved what

A person's decision is a record in the journal with their name and how speccoder knows it (`asserted` at a terminal or the CLI, `connected` from an editor over ACP), bound to the digest of the call they were shown: an approval of anything else is refused. Their answers enter the run labelled as a person's input, not as speccoder's own. Calls a rule decides name the rule; an expired call names no one. See [approvals](@/docs/approvals.md#who-decided).

## The policy

Every tool call is classified before it runs.

**Denied — no approval lifts a denial:**
- writing the spec (`.specify/`, `specs/`) or anything that decides verification: `speccoder.toml`, CI configuration (`.github/`, `.gitlab-ci.yml`), test-runner and build configuration (`.cargo/config*`, `.config/nextest.toml`, `pytest.ini`, `tox.ini`, jest/vitest configs, `justfile`, `Makefile`);
- writing agent configuration (`.claude/`, `.agents/`, `.codex/`, `.gemini/`, `.opencode/`, `.cursor/`, `.kiro/` and others, `.mcp.json`, `opencode.json`): hooks, settings, skills and MCP servers there run code in a later agent session — including an external executor's — outside speccoder's sandbox;
- anything under `.git`;
- reading or writing secret-looking files (`.env*`, `*.pem`, `*.key`, SSH keys, `.netrc`, `.npmrc`, `.pypirc`);
- paths outside the worktree, symlink escapes included;
- privilege escalation (`sudo`, `su`, `doas`, `pkexec`), git remotes (`push`, `fetch`, `pull`, `clone`, `remote`, `submodule`) and destructive git (`clean`, `reset --hard`, `filter-branch`).

**Needs a person:** agent context files (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`); dependency manifests; tests that existed before the attempt; shells, `env`, `xargs`, `eval`; network tools; recursive `rm`; git commands that change state; unlisted commands; MCP tools not in `allow`; any command that asks for the network.

**Allowed:** reading the project, writing ordinary source files, the configured checks and `[commands] allow`. A check that starts with options (`python3 -m pytest …`) allows exactly that command, not its bare program.

Three denied calls in a row end an attempt.

Tests run code the model wrote, which no tool-level check can see. So after every command speccoder runs, it re-checks these protected inputs — files git ignores included — and a command that changed one ends the attempt immediately with an integrity finding.

## The OS sandbox

The policy decides which commands run; the sandbox confines what they do. Every command speccoder starts for a task — checks, the model's commands, the formatter, acceptance tests, mutation tools, MCP servers — runs inside it:

- **macOS:** Seatbelt, with a profile generated per run.
- **Linux:** Landlock for the filesystem and a seccomp filter for the network, applied with `no_new_privs` by speccoder's own helper before the command starts. Needs a kernel with Landlock (5.13+).

Inside, a command:

- **writes** only to the worktree, a private temporary directory and the package caches builds need (`~/.cargo/registry`, `~/.cache`, `~/.npm`, `~/go/pkg/mod`, `~/Library/Caches`, …) — not to speccoder's state, not to `~/.cargo/bin`, not to your checkout;
- **cannot read** credentials: `~/.ssh`, `~/.aws`, `~/.azure`, `~/.gnupg`, `~/.netrc`, `~/.docker`, `~/.kube`, `~/.npmrc`, `~/.pypirc`, `~/.git-credentials`, `~/.config/gh`, `~/.config/gcloud`, `~/.cargo/credentials`, keyrings;
- **has no network** beyond loopback, so local test servers work (on Linux loopback is blocked too). Unix sockets work only inside the writable trees, so the Docker socket is out of reach;
- on macOS **cannot open applications** or send Apple events, which would run code outside the sandbox.

A command gets the network only when its prefix is in `[sandbox] network_allow`, or when a person approves that exact call. Fetch dependencies before a run (`cargo fetch`, `npm ci`), or allow the fetching command. An escape suite checks these properties on macOS and Linux in CI.

Where the OS has no sandbox, every run says `commands run unsandboxed`; `[sandbox] mode = "required"` stops instead.

## Secrets

- API keys are read only from the environment, and never written to the journal or the output.
- Every command speccoder starts gets an environment without secret-looking variables: `*_API_KEY`, `*_TOKEN`, `*SECRET*`, `*PASSWORD*`, `AWS_*`, `ANTHROPIC_*`, `OPENAI_*`, `SSH_AUTH_SOCK`, `GIT_ASKPASS`, `SSH_ASKPASS`.
- Commands run with stdin closed, a deadline for their whole process group, and capped output.

## Provenance labels

speccoder runs on the [agentplane](https://crates.io/crates/agentplane) runtime, whose gates decide by **labels**: where a piece of content came from. speccoder labels everything the model reads by its real source, where it enters:

- **trusted:** speccoder's own instructions, and your committed spec, plan, notes and decisions;
- **untrusted, by source:** check output quoted in a retry, worktree content, MCP tool descriptions and results, speccoder's own tool results, and the model's answers.

Tool results reach the model through the runtime's conversation record, so every model call carries the label of everything the conversation has read, and a refusal speccoder sends back carries the label of the call it quotes. A mutating call's arguments may only come from the model, speccoder's tools, the verifier, the worktree and your configured MCP tools; any other source is refused. Labels are never inferred from content, so a source nobody anticipated is never trusted by default.

## External agents

With `[executor] kind = "claude"`, `"codex"` or `"command"`, another agent makes the attempt with its own tools. speccoder's policy cannot refuse its individual calls, and it runs outside speccoder's sandbox with your full environment, because it needs its own credentials; its own permission system is its boundary. The worktree, the diff audit, the checks and the verdict still apply afterwards.

## What it does not protect against

- **Code that runs during checks, beyond the sandbox:** it can read files outside the hidden paths, use CPU and memory, and write to package caches.
- **MCP servers:** speccoder gates the calls; what a server does when called is up to the server. Configure only servers you trust.
- **What you send to the model provider:** your spec, plan, the files the model reads, command output and diffs go to the configured provider. Do not run speccoder on code you may not share with it.
- **Prompt injection:** text in the repository or in tool output can try to steer the model. The policy, the sandbox and the verifier limit what that can achieve; they do not prevent the attempt.
- **Merging unreviewed work:** `verified` is evidence, not a review.

## Running it safely

1. Keep the sandbox on — `[sandbox] mode = "required"` makes sure — and prefer a container, VM or dedicated account without your personal credentials.
2. Keep API keys in the environment and use one with a spending limit.
3. Review `speccoder.toml` after `speccoder init`, especially `[commands] allow` and `[mcp]`. Keep allowances narrow.
4. For unattended runs use `--no-prompt`: calls that need approval stop instead of waiting.
5. Read `speccoder report` and the branch's diff before merging.

To report a vulnerability, use [GitHub's private security advisories](https://github.com/hupe1980/speccoder/security/advisories/new).
