+++
title = "Integrations"
description = "Use speccoder from Zed and JetBrains over the Agent Client Protocol, give it MCP tools, let Claude Code or Codex make the attempts, hand off draft pull requests, and run it in CI."
weight = 80

[extra]
group = "Use"
+++

## Editors (ACP)

`speccoder acp` speaks the [Agent Client Protocol](https://agentclientprotocol.com) on stdio, so editors with ACP agents — Zed, JetBrains IDEs and others — can drive speccoder. In Zed:

```json
{
  "agent_servers": {
    "speccoder": { "command": "speccoder", "args": ["acp"] }
  }
}
```

Slash commands:

| Command | Does |
|---|---|
| `/run`, `/run T005`, `/run --converge`, `/run --parallel 2` | Run the ready tasks, or one |
| `/converge` | Assess the finished feature against its spec and append what remains |
| `/inbox` | What waits for your decision, in every feature |
| `/report`, `/status`, `/cost` | The evidence, the current or last run, tokens and money |
| `/specify <what to build>`, `/clarify`, `/plan`, `/tasks`, `/analyze` | Spec Kit's planning skills |

- Tasks and the model's tool calls appear as the editor's tool calls; during `/run` the feature's tasks are the editor's plan, updated as they finish.
- A call that needs a person becomes the editor's permission dialog, with the terminal prompt's content (the diff, why it stopped, the sandbox, the model's claim) and the choices once, for the task, for the run, reject and stop. Rejecting asks for a reason in a form when the editor can show one; dismissing the dialog stops the task.
- Cancelling stops a feature run before its next task.
- The model and key come from `speccoder.toml` and the environment, as on the command line.

## MCP servers

Give the model tools from any stdio [Model Context Protocol](https://modelcontextprotocol.io) server:

```toml
[mcp.notes]
command = ["notes-server", "--stdio"]
allow = ["list_notes"]        # called without approval
deny = ["delete_everything"]  # never offered
```

Servers start in the worktree for each task, inside the sandbox, with a scrubbed environment; their tools appear to the model as `mcp__<server>__<tool>`. Every other call needs approval, and whatever a tool writes is still caught by the diff audit and the verifier.

**Allowed tools stay what you reviewed.** The first time speccoder sees an allowed tool, it records a digest of its name, description, schema and annotations. If the server later changes any of that, the run warns and every call of the tool needs approval until you approve one — a changed description is how a server would smuggle new instructions to the model.

## External agents

Let another coding agent make the attempts while speccoder judges them:

```toml
[executor]
kind = "claude"      # Claude Code (claude -p); "codex" (codex exec); "command"
```

| `kind` | Runs |
|---|---|
| `claude` | Claude Code with `acceptEdits`, file tools, Bash only for your checks and `[commands] allow`, no web tools |
| `codex` | Codex with `--sandbox workspace-write` |
| `command` | the program in `argv`: the task on stdin, working in the current directory, its summary on stdout |

`model` and `timeout_secs` are optional. Log in to the agent first. The agent gets the context a native attempt gets (task, story, plan, constitution, the verifier's facts after a failure) and the rules speccoder will check; a final message starting with `BLOCKED:` is an honest stop.

What stays: the isolated worktree, the acceptance tests written before the attempt (by the model in `[model]`), the diff audit, the checks and the verdict, retries, the monitor, ticking and committing. What changes: **guarantees are diff-level, not call-level.** The agent runs outside speccoder's sandbox under its own permission system, so there are no approvals during an attempt; a frozen test it edits is caught by the audit afterwards and fails the verdict.

## Pull requests

`speccoder pr` hands a feature off as a draft pull request whose description is the evidence; see [GitHub](@/docs/github.md#its-pull-request).

## CI and scripting

Three contracts, all described by published [JSON Schemas](@/docs/cli.md#json-output):

- `--json` prints one result object: status, exit code, a [code](@/docs/codes.md) with `fix` and `docs` when something is wrong, `next`, and the typed `data`.
- `--events` streams the work as JSON Lines — tasks, stages, tool calls, approvals, usage — and ends in the same object ([events](@/docs/cli.md#events)).
- Exit codes are stable ([CLI reference](@/docs/cli.md#exit-codes)); 23 always means "a person is needed".

For unattended runs use `--no-prompt`: calls that need approval stop the task instead of waiting.

### GitHub Actions

Under `GITHUB_ACTIONS=true`, `run` and `report` append the feature's report as Markdown to the job summary and annotate tasks: a warning for `regression only` and suspected gaming, an error for failed and blocked tasks. speccoder stays within GitHub's limits — ten annotations of a kind per step (a notice counts the rest), a summary under 1 MiB (least important sections left out, and said so). The model's own words appear in neither.

```yaml
- run: speccoder run --no-prompt
  env:
    ANTHROPIC_API_KEY: …   # from the repository's secrets
```

`speccoder report --markdown` prints the same Markdown anywhere, for a pull request comment or a chat.

## Spec Kit's `bugfix` workflow

With Spec Kit's first-party `bug` extension and its `bugfix` workflow installed, `speccoder workflow install --overlay bugfix` writes a project overlay (`.specify/workflows/overlays/bugfix/speccoder.yml`) that replaces the workflow's `test` step with speccoder's (`speccoder bug run --slug {{ inputs.slug }}`): `specify workflow run bugfix` assesses with the extension, pauses for the reproductions' confirmation, and reports the verdict. `speccoder workflow uninstall --overlay bugfix` removes it. See [bug fixes](@/docs/bugs.md).
