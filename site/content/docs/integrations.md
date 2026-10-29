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

Let another coding agent make the attempts while speccoder decides its calls and judges the result:

```toml
[executor]
agent = "copilot"    # or "opencode", "claude", "codex", "gemini"
```

| `agent` | Starts (over the [Agent Client Protocol](https://agentclientprotocol.com)) |
|---|---|
| `copilot` | GitHub Copilot: `copilot --acp`, else `npx -y @github/copilot@1.0.105 --acp` |
| `opencode` | OpenCode: `opencode acp` |
| `claude` | Claude Code: `npx -y @agentclientprotocol/claude-agent-acp@0.98.0` |
| `codex` | Codex: `npx -y @agentclientprotocol/codex-acp@2.3.0` |
| `gemini` | Gemini CLI: `npx -y @google/gemini-cli@0.66.0 --acp` |

- Any other ACP agent: `kind = "acp"` with `argv = ["my-agent", "--acp"]`. A program that is not an ACP agent: `kind = "command"` with `argv` — the task on stdin, its summary on stdout, judged from the diff alone.
- `model` selects the agent's model where it offers a choice; `mode` keeps a permission mode of your choosing; `timeout_secs` (default `1800`) bounds one attempt — then speccoder cancels the turn and stops the agent.
- `speccoder run --agent opencode` (and the app's Run ▾ menu) picks the agent for one run; `speccoder.toml` is unchanged. `speccoder doctor --agents` and Settings › Executor show each agent as *installed*, *signed in* and *asks first*, with the one step that fixes a missing tick. speccoder never signs an agent in: a sign-in it needs stops the task with [`EXECUTOR-LOGIN`](@/docs/codes.md#executor-login) and the agent's own login command.

**What speccoder governs.** Before the first prompt speccoder puts the agent into its own ask-first permission mode (recorded per adapter version; OpenCode is started with every permission rule set to ask, merged into an `OPENCODE_CONFIG_CONTENT` of yours), unless you set `mode`. Every permission request the agent sends is then decided as a [native attempt's call](@/docs/approvals.md), on every path and command it names: **a command the agent runs itself always asks you** — it runs outside the sandbox, so no rule allows it; allow it for the task and that exact command runs on without asking again. Plain reads (`cat`, `head`, `tail`, `wc`, `ls` of files inside the worktree, never a credential or a file copied from your checkout) count as reads; a file by the protected paths; a call speccoder cannot classify by a person; a fetch is refused. Allowed calls get the agent's *allow once*, never *always*; a call that needs you waits at the terminal, in Needs You, or for `task approve`. Files and commands the agent runs through speccoder's file and terminal access go through the workspace tools, the protected paths, the secret masking, the command policy and the OS sandbox. A person's time deciding does not count against `timeout_secs`.

**What stays outside.** The agent runs as its own process with your environment, outside speccoder's sandbox. What it does without asking never reaches speccoder: some agents read files unasked, and **an allow rule in the agent's own settings** (Claude Code's `permissions.allow`, also in a repository's `.claude/settings.json`; Gemini's `tools.allowed`; Copilot's allowed tools) lets it skip asking even in its ask-first mode. speccoder checks afterwards: an edit or command the agent reports without having asked marks the run *decides alone*, with the count. A move to a mode that decides alone cancels the turn. When no recording says whether this version asks, the run says *not known whether it asks*. The worktree, the acceptance tests written before the attempt (by the model in `[model]`), the diff audit, the checks and the verdict, retries, the monitor, ticking and committing are the same as for every attempt. A final message starting with `BLOCKED:`, or a turn that changed nothing, is an honest stop. See [security](@/docs/security.md#external-agents).

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
