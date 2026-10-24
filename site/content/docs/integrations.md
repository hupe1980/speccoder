+++
title = "Integrations"
description = "Use speccoder from Zed and JetBrains over the Agent Client Protocol, give it MCP tools, let Claude Code or Codex make the attempts, hand off draft pull requests, and run it in CI."
weight = 80

[extra]
group = "Use"
+++

## Editors (ACP)

`speccoder acp` speaks the [Agent Client Protocol](https://agentclientprotocol.com) on stdio, so editors that support ACP agents — Zed, JetBrains IDEs and others — can drive speccoder. In Zed:

```json
{
  "agent_servers": {
    "speccoder": { "command": "speccoder", "args": ["acp"] }
  }
}
```

speccoder's commands are offered as slash commands: `/run`, `/run T005`, `/run --converge`, `/run --parallel 2`, `/converge`, `/report`, `/status`, `/cost`, and the planning skills `/specify <what to build>`, `/clarify`, `/plan`, `/tasks`, `/analyze`. Tasks and the model's tool calls appear as tool calls in the editor; a call that needs a person becomes the editor's permission dialog with the same question as the terminal prompt — a diff of the file, why it stopped, the sandbox, the model's claim — and the choices once, for the task, for this run (not for high-risk calls), reject and stop. A rejection asks for the reason in a form when the editor can show one; dismissing the dialog stops the task, it never rejects. During `/run` the feature's tasks appear as the editor's plan, updated as they finish. Cancelling stops a feature run before its next task. The model and key come from `speccoder.toml` and the environment, as on the command line.

## MCP servers

Give the model tools from any stdio [Model Context Protocol](https://modelcontextprotocol.io) server:

```toml
[mcp.notes]
command = ["notes-server", "--stdio"]
allow = ["list_notes"]        # called without approval
deny = ["delete_everything"]  # never offered
```

Servers start in the worktree for each task, inside the sandbox, with a scrubbed environment. Their tools appear to the model as `mcp__<server>__<tool>`. speccoder cannot know what an MCP tool does, so every call needs approval unless allowed — and whatever a tool writes is still caught by the diff audit and the verifier.

**Allowed tools stay what you reviewed.** The first time speccoder sees a tool you allow, it records a digest of its name, description, schema and annotations. If the server later changes any of that, the next run warns and every call of the tool needs approval until you approve one. A changed description is how a server would smuggle new instructions to the model.

## External agents

Let another coding agent make the attempts and keep speccoder's judgement:

```toml
[executor]
kind = "claude"      # Claude Code (claude -p); or "codex" (codex exec), or "command"
```

The agent gets the same context a native attempt gets — task, story, plan, constitution and, after a failure, the verifier's facts — and the rules speccoder will check. A final message starting with `BLOCKED:` is an honest stop.

What stays: the isolated worktree, the acceptance tests written before the attempt, the diff audit, the checks and the verdict, retries, the monitor, ticking and committing. What changes: **guarantees are diff-level, not call-level.** speccoder cannot classify the agent's individual calls, so there are no approvals during an attempt; a frozen test the agent edits is caught by the audit afterwards and fails the verdict. The agent runs outside speccoder's sandbox, under its own permission system: Claude Code with `acceptEdits`, file tools, Bash only for your checks and `[commands] allow`, and no web tools; Codex with `--sandbox workspace-write`. Log in to the agent first. The model in `[model]` still writes the acceptance tests.

`kind = "command"` runs any program: it reads the task on stdin, works in the current directory and prints its summary on stdout.

## Pull requests

```sh
speccoder pr --dry-run          # title, body and commands; nothing pushed
speccoder pr                    # shows that, then asks before pushing
speccoder pr --yes --base main  # without asking
```

`speccoder pr` pushes `speccoder/<feature>` to `--remote` (default `origin`) and opens a **draft** pull request with the GitHub CLI against your checkout's branch or `--base`. The description is the evidence: every task with its verdict, mutation results and commit, what is still open, tokens, and what `verified` does not mean. It is the only command that talks to a git remote; it asks first, is a dry run without a terminal unless you pass `--yes`, and never merges or marks a pull request ready.

## CI and scripting

Three contracts, all described by published [JSON Schemas](@/docs/cli.md#json-output):

- `--json` prints one result object: status, exit code, a [code](@/docs/codes.md) with `fix` and `docs` when something is wrong, `next`, and the typed `data`.
- `--events` streams the work as JSON Lines — tasks, stages, tool calls, approvals, usage — and ends in the same object; follow a 30-minute run as it happens ([events](@/docs/cli.md#events)).
- Exit codes are stable ([CLI reference](@/docs/cli.md#exit-codes)); 23 always means "a person is needed".

For unattended runs use `--no-prompt`: calls that need approval stop the task instead of waiting.

### GitHub Actions

Nothing to configure: under `GITHUB_ACTIONS=true`, `run` and `report` append the feature's report as Markdown to the job summary, and annotate tasks — a warning for `regression only` and suspected gaming, an error for failed and blocked tasks. GitHub shows ten annotations of a kind per step; speccoder stays inside that and adds a notice counting the rest, and keeps the summary under GitHub's 1 MiB limit by leaving out the least important sections (and saying so). The model's own words never appear in either.

```yaml
- run: speccoder run --no-prompt
  env:
    ANTHROPIC_API_KEY: …   # from the repository's secrets
```

`speccoder report --markdown` prints the same Markdown anywhere — for a pull request comment or a chat.
