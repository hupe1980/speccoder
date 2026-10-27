+++
title = "Security and the sandbox"
description = "speccoder's security model: an isolated worktree, an OS sandbox on macOS and Linux, a policy that classifies every call, secret scrubbing, provenance labels — and what it does not protect against."
weight = 90

[extra]
group = "Trust"
nav = "Security and the sandbox"
+++

## The threat model in one sentence

**Your checks run code the model wrote**, possibly after it read text an attacker controls (a dependency's README, an issue, a tool's output), so speccoder treats the code it runs as untrusted:

| Layer | Decides | Where |
|---|---|---|
| Isolated worktree | your checkout is never touched | all work |
| Policy | *which* calls may run | every tool call, before it runs |
| OS sandbox | *what* a running command can reach | every command speccoder starts |
| Secret scrubbing | which environment a command sees | every command speccoder starts |
| Provenance labels | what may steer a mutating call | every model call |
| Verifier and audit | whether a task is done | after every attempt |

## Isolation

All work happens on `speccoder/<feature>` in a worktree under speccoder's state directory, and reaches your branch only when you merge it. speccoder never fetches or pushes on its own; `speccoder pr` pushes after you confirm. The model writes in your checkout only through a hosted planning skill (`speccoder spec …`), which may write only the format's planning artifacts and leaves them uncommitted.

### In-place mode

[`--in-place`](@/docs/running.md#in-place-runs) runs the attempt in your checkout. It needs a clean checkout and stops when HEAD, the branch, the tree, or any spec, plan, task list, constitution, check or agent-configuration file (git-ignored ones included) changes other than by its own tick. Instead of a worktree's isolation:

- **No git that moves the checkout.** The model's `git checkout`, `switch`, `reset`, `commit`, `merge`, `rebase`, `cherry-pick`, `revert`, `stash`, `branch`, `tag`, `update-ref`, `worktree`, `am`, `apply --index` and `restore --staged` are denied.
- **Only what a worktree would hold.** The file tools refuse files git ignored at the start (a journaled list), except the spec, plan and tasks, which stay readable and never writable.
- **`.git` read-only, ignored secrets unreadable.** macOS: Seatbelt rules. Linux: each command starts in its own user and mount namespace with `.git` bound read-only and git-ignored credential-like files (`.env`, keys) reading as empty.
- **A guard where the sandbox cannot.** Some hosts forbid user namespaces (a container's default seccomp profile, Ubuntu's AppArmor restriction); the run says so at the start. In every case speccoder hashes `.git`'s `config`, `hooks/`, `info/` and every branch and tag but its own, and stops before the next tick when they change (`GIT-CHANGED`). speccoder's own git calls never run hooks or an fsmonitor.

Not confined in place:

- a child process can read git-ignored files that do not look like credentials (the file tools cannot);
- without user namespaces, a command can write `.git` (the guard catches it) and read ignored secrets (nothing catches it);
- edits you make during an attempt are judged as the agent's.

A fresh clone holds the code and the evidence commits, but not an ignored `tasks.md` or its ticks.

## The policy

Every tool call is classified before it runs.

**Denied** (no approval lifts a denial):
- writing the spec (`.specify/`, `specs/`, `openspec/`) or anything that decides verification: `speccoder.toml`, CI configuration (`.github/`, `.gitlab-ci.yml`), test-runner and build configuration (`.cargo/config*`, `.config/nextest.toml`, `pytest.ini`, `tox.ini`, jest/vitest configs, `deny.toml`, `justfile`, `Makefile`);
- writing agent configuration (`.claude/`, `.agents/`, `.codex/`, `.gemini/`, `.opencode/`, `.cursor/`, `.kiro/` and others, `.mcp.json`, `opencode.json`), whose hooks, settings, skills and MCP servers run code in a later agent session, outside the sandbox;
- anything under `.git`;
- reading or writing secret-looking files (`.env*`, `*.pem`, `*.key`, SSH keys, `.netrc`, `.npmrc`, `.pypirc`);
- paths outside the worktree, symlink escapes included;
- privilege escalation (`sudo`, `su`, `doas`, `pkexec`), git remotes (`push`, `fetch`, `pull`, `clone`, `remote`, `submodule`), destructive git (`clean`, `reset --hard`, `filter-branch`), and writing git notes (attestations are speccoder's).

**Needs a person** ([approvals](@/docs/approvals.md)): agent context files (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`); dependency manifests; tests that existed before the attempt; shells, `env`, `xargs`, `eval`; network tools; recursive `rm`; git commands that change state; unlisted commands; MCP tools not in `allow`; any command that asks for the network.

**Allowed:** reading the project, writing ordinary source files, the configured checks, and the [allow lists](@/docs/approvals.md#allow-lists).

**Protected paths cannot be renamed around:**

- They are matched in any letter case, on every OS: on a case-insensitive volume `Speccoder.toml` and `.CLAUDE/settings.json` are the protected files.
- A write is judged on the file it reaches: symlinks of existing directories are resolved (they must stay inside the worktree), and the file itself is never written through a symlink.
- A write whose spelling reaches another file (a symlinked directory, another letter case) is refused unless it names that file.

Approvals stay narrow: a grant covers one resolved object and ends with its run, copied arguments can be approved once only, and a run prompts a bounded number of times ([what a decision covers](@/docs/approvals.md#what-a-decision-covers)). Three denied calls in a row end an attempt. After every command speccoder re-checks the protected inputs, git-ignored ones included: a command that changed one ends the attempt with an integrity finding.

## The OS sandbox

Every command speccoder starts for a task (checks, the model's commands, the formatter, acceptance tests, mutation tools, MCP servers) runs inside it:

- **macOS:** Seatbelt, with a profile generated per run.
- **Linux:** Landlock for the filesystem and a seccomp filter for the network, applied with `no_new_privs` by speccoder's own helper before the command starts. Needs Landlock ABI 3 (Linux 6.2 or later).

Inside, a command:

- **writes** only to the worktree, a private temporary directory and the package caches builds need (`~/.cargo/registry`, `~/.cache`, `~/.npm`, `~/go/pkg/mod`, `~/Library/Caches`, …), plus `[sandbox] writable`; not to speccoder's state, `~/.cargo/bin` or your checkout;
- **cannot read** credentials: `~/.ssh`, `~/.aws`, `~/.azure`, `~/.gnupg`, `~/.netrc`, `~/.docker`, `~/.kube`, `~/.npmrc`, `~/.pypirc`, `~/.git-credentials`, `~/.password-store`, `~/.config/gh`, `~/.config/gcloud`, `~/.cargo/credentials`, keyrings and keychains;
- **has no network** beyond loopback (`[sandbox] network = "loopback"`, the default), so local test servers work; on Linux loopback is blocked too;
- **cannot connect to a Unix socket outside the writable paths** (the Docker daemon's, a database's, an agent's); a check that needs one lists its directory in `[sandbox] writable`;
- on macOS **cannot open applications** or send Apple events.

A command gets the network only when it is exactly in `[sandbox] network_allow`, extends a `[sandbox] network_allow_prefix` entry, or a person approves that exact call. Fetch dependencies before a run (`cargo fetch`, `npm ci`), or allow the fetching command. CI runs an escape suite against these properties on macOS and Linux.

Where the OS has no sandbox, every run says `commands run unsandboxed`; with `[sandbox] mode = "required"` it stops instead (`SANDBOX-UNAVAILABLE`).

**Ubuntu 23.10 and later** restrict unprivileged user namespaces through AppArmor. Landlock and seccomp still apply, but without the namespace a command can write `.git` (the [git guard](#in-place-mode) catches it), read git-ignored secrets and reach daemon sockets; every run says so. To give speccoder its namespace, allow it in an AppArmor profile (the path is where your `speccoder` is installed):

```text
# /etc/apparmor.d/speccoder — then: sudo apparmor_parser -r /etc/apparmor.d/speccoder
abi <abi/4.0>,
include <tunables/global>
profile speccoder /usr/local/bin/speccoder flags=(unconfined) {
  userns,
}
```

## Held-out tests

Held-out tests are sealed in the state directory ([where they are kept](@/docs/evidence.md#where-they-are-kept)). The web interface's file view serves project paths only, follows no symbolic link, and reads an untracked file only when its real path is inside the feature's worktree and no held-out set is overlaid; the diff view shows only commits a branch, `HEAD` or a saved attempt reaches. Comments, revisions, nudges and every author call but the held-out author's own never carry held-out material.

## Secrets

- API keys are read only from the environment, and never written to the journal or the output. Bedrock's AWS credentials (an API key, an `aws login` or SSO session, a profile) are loaded by speccoder's own process for its model calls; `doctor` and `init` name their source, never a value.
- Every command speccoder starts gets an environment without secret-looking variables: `*_API_KEY`, `*_TOKEN`, `*SECRET*`, `*PASSWORD*`, `AWS_*`, `ANTHROPIC_*`, `OPENAI_*`, `SSH_AUTH_SOCK`, `GIT_ASKPASS`, `SSH_ASKPASS`. Colour is off for these commands (`NO_COLOR=1`, `CARGO_TERM_COLOR=never`).
- Commands run with stdin closed, a deadline for their whole process group, and capped output.
- A git-ignored secret file [copied into a worktree](@/docs/running.md#copied-files) (`[worktree] include`, `.worktreeinclude`) stays unreadable to the model's file tools. Its values (8 characters or more, not a plain number) are masked as `[redacted:NAME]`, also base64-, hex- and URL-encoded, in command and check output, tool results, the journal and everything shown; a commit never holds the file, and a diff adding a value stops the task (`SECRET-IN-DIFF`). A value transformed otherwise is not recognised.
- Signing keys: see [what a signature proves](@/docs/attestations.md#what-a-signature-proves).

## Provenance labels

speccoder runs on the [agentplane](https://crates.io/crates/agentplane) runtime, whose gates decide by **labels**: where content came from, set where it enters, never inferred from content. Trusted are speccoder's own instructions and your committed spec, plan, notes and decisions; untrusted are check output, worktree content (`AGENTS.md` included), MCP tool descriptions and results, tool results and the model's answers. Every model call carries the labels of everything its conversation has read, and a mutating call's arguments may come only from the model, speccoder's tools, the verifier, the worktree and your configured MCP tools.

## External agents

With an [`[executor]`](@/docs/integrations.md#external-agents), another agent makes the attempt with its own tools, outside speccoder's sandbox and with your environment (it needs its own credentials). speccoder's policy cannot refuse its individual calls; its own permission system is the boundary. The worktree, the diff audit, the checks and the verdict still apply.

## Checking someone else's evidence

[`speccoder attest verify --branch`](@/docs/verifier.md) is built to run on a pull request you do not trust: it executes nothing from the checked tree and takes trust only from you. The [action](@/docs/github.md#evidence-check) adds trust from a commit the checked one did not write and refuses to run with the head checked out under `pull_request_target`.

## What it does not protect against

- **Code that runs during checks, within the sandbox's limits:** it can read files outside the hidden paths, use CPU and memory, and write to package caches.
- **MCP servers:** speccoder gates the calls; what a server does when called is up to the server. Configure only servers you trust.
- **What you send to the model provider:** your spec, plan, the files the model reads, command output and diffs. Do not run speccoder on code you may not share with it.
- **Prompt injection:** text in the repository or in tool output can try to steer the model. The policy, the sandbox and the verifier limit what that achieves; they do not prevent the attempt.
- **Merging unreviewed work:** `verified` is evidence, not a review.

## Running it safely

1. Keep the sandbox on (`[sandbox] mode = "required"`), and prefer a container, VM or dedicated account without your personal credentials.
2. Use an API key with a spending limit.
3. Review `[commands]`, `[sandbox] network_allow*` and `[mcp]` after `speccoder init`; keep allowances narrow.
4. Use `--no-prompt` for unattended runs.
5. Read `speccoder report` and the branch's diff before merging.

To report a vulnerability, use [GitHub's private security advisories](https://github.com/hupe1980/speccoder/security/advisories/new).
