+++
title = "Integrations"
description = "Use speccoder from Zed and JetBrains over the Agent Client Protocol, give it MCP tools, let Claude Code or Codex make the attempts, run it in CI, and as a Spec Kit workflow step."
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

| Slash command | Does |
|---|---|
| `/run`, `/run T005`, `/run --converge`, `/run --parallel 2` | Run the ready tasks, or one |
| `/converge` | Assess the finished feature against its spec and append what remains |
| `/inbox` | What waits for your decision, in every feature |
| `/report`, `/status`, `/cost` | The evidence, the current or last run, tokens and money |
| `/specify <what to build>`, `/clarify`, `/plan`, `/tasks`, `/analyze` | Spec Kit's planning skills |

- Tasks and the model's tool calls appear as the editor's tool calls; during `/run` the feature's tasks are the editor's plan.
- A call that needs a person becomes the editor's permission dialog, with the [inline prompt](@/docs/approvals.md#inline)'s content and choices. Rejecting asks for a reason in a form when the editor can show one; dismissing the dialog stops the task.
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

- Servers start in the worktree for each task, inside the sandbox, with a scrubbed environment; their tools appear to the model as `mcp__<server>__<tool>`.
- Every call of a tool not in `allow` needs approval; whatever a tool writes is still caught by the diff audit and the verifier.
- **Allowed tools stay what you reviewed.** The first time speccoder sees an allowed tool it records a digest of its name, description, schema and annotations. If the server changes any of that, the run warns and each call of the tool needs approval until a person approves one.

## External agents

Let another coding agent make the attempts while speccoder judges them:

```toml
[executor]
kind = "claude"      # or "codex", "command"
```

| `kind` | Runs |
|---|---|
| `claude` | Claude Code (`claude -p`) with `acceptEdits`, its file tools, Bash only for your checks and `[commands] allow` (passed as exact Claude Code `Bash(…)` rules), no web tools |
| `codex` | `codex exec --sandbox workspace-write` |
| `command` | the program in `argv`: the task on stdin, working in the current directory, its summary on stdout |

- `model` and `timeout_secs` (default `1800`) are optional. Log in to the agent first.
- The agent gets the context a native attempt gets (task, story, plan, constitution, the verifier's facts after a failure) and the rules speccoder will check. A final message starting with `BLOCKED:` is an honest stop.
- What stays: the isolated worktree, the acceptance tests written before the attempt (by the model in `[model]`), the diff audit, the checks and the verdict, retries, the monitor, ticking and committing.
- What changes: **guarantees are diff-level, not call-level.** The agent runs outside speccoder's sandbox under its own permission system, so there are no approvals during an attempt; a frozen test it edits is caught by the audit and fails the verdict. See [security](@/docs/security.md#external-agents).

## CI and scripting

`--json` (one result object), `--events` (JSON Lines ending in that object) and the exit codes are stable contracts with published schemas; see the [CLI reference](@/docs/cli.md#json-output). Exit 23 always means "a person is needed". For unattended runs use `--no-prompt`: a call that needs approval waits for [`task approve`](@/docs/approvals.md#later) instead of prompting.

### GitHub Actions

Under `GITHUB_ACTIONS=true`, `run` and `report` append the feature's report as Markdown to the job summary and annotate tasks: a warning for `regression only` and suspected gaming, an error for failed and blocked tasks. They stay within GitHub's limits: ten annotations of a kind per step (a notice counts the rest), a summary under 1 MiB (the least important sections left out, and said so). The model's own words appear in neither.

```yaml
- run: speccoder run --no-prompt
  env:
    ANTHROPIC_API_KEY: …   # from the repository's secrets
```

`speccoder report --markdown` prints the same Markdown anywhere. Pull requests: [GitHub](@/docs/github.md#its-pull-request); checking evidence on them: [the evidence check](@/docs/github.md#evidence-check). Spec Kit workflow steps: [planning](@/docs/planning.md#spec-kit-workflows).
