+++
title = "Hooks"
description = "Your own commands around a run: a gate that can stop a tick or a pull request, an observer that hears every tick and stop. Trusted by digest, never able to pass a task."
weight = 75

[extra]
group = "Use"
+++

A hook is a command of yours that speccoder runs at a fixed point. There are two kinds:

- A **gate** can stop a tick or a pull request. Only exit 0 lets it through.
- An **observer** hears what speccoder decided. It never changes the outcome.

No hook can tick a task, approve a call or reach the model.

```toml
# A gate: no task is ticked while the licence check objects.
[hooks.licence]
at = "tick"
argv = ["cargo", "deny", "check", "licenses"]
network = true

# An observer: a line in the team channel for every tick and stop.
[hooks.team-chat]
on = ["task.ticked", "task.stopped"]
url = "https://hooks.example.com/speccoder"
secret_env = "SPECCODER_HOOK_SECRET"
```

`speccoder hooks` lists them. With none declared, it prints these two to copy. Every key is listed under [`[hooks.<name>]`](@/docs/configuration.md#hooks-name).

## Trust

A hook runs only after you trusted its exact bytes. That covers its definition, every file in the repository its `argv` names, and the files it lists in `files`. List there what a gate reads besides its own script, such as `deny.toml`:

```toml
[hooks.licence]
at = "tick"
argv = ["cargo", "deny", "check", "licenses"]
files = ["deny.toml"]
```

```console
$ speccoder hooks trust licence
licence — a gate that stands before tick

  [hooks.licence]
  argv = ["sh", "scripts/licence.sh"]
  at = "tick"
  runs scripts/licence.sh  sha256:3f9a…
  digest sha256:8f32…

Trust licence to run on this machine? [y/N] y
```

Your trust is recorded in speccoder's state on this machine, never in the checkout. A change of one byte makes the hook untrusted again; trusting it then shows what it was and what it is now. Until it is trusted:

- a gate blocks: the tick waits, and Needs You asks you to trust it;
- an observer is skipped, and the run says so once.

The model cannot write `speccoder.toml`, a file a hook runs or a file it lists. Like the checks, gates come from the `speccoder.toml` of the branch being ticked or pushed.

In CI, pass the digest that `speccoder hooks` prints:

```console
speccoder run --trust-hook licence=sha256:8f32…
```

It is accepted only if it matches the hook's current digest.

## Gates

A gate stands at one of two points:

- `at = "tick"`: after the verdict, before the tick is committed. This covers a task you accept by hand too.
- `at = "pr"`: after you confirm a pull request, before anything is pushed.

A gate runs where the checks run: the same worktree, the same sandbox. It may write output that git ignores, like `target/`. It cannot change a file the tick would commit, read speccoder's state or the held-out tests, or reach the network unless you set `network = true`. A pull request's gates see exactly the commit that is pushed.

The rule is the one CI uses, so an existing check script works as a gate unchanged:

| The gate | The point |
|---|---|
| exits 0 | passes |
| exits with anything else | blocked, with the last line it printed |
| crashes, is killed, times out, cannot start | blocked, with the cause |
| changes a file the tick would commit | blocked |
| is not trusted, or no sandbox is enforced | blocked |

A blocked tick is not committed. The attempt is saved, and the verdict it earned is kept on it. Needs You shows *T004 blocked by licence* in the gate's own words, with **Run again** and **Test**. The words go to you, never to the model: they are kept in speccoder's state, not in git.

There is no flag to skip a gate.

Gates at one point run in name order; the first block stops the rest. A tick names the gates it passed (`Gate: licence passed`), and `report` shows them.

A gate is told where it stands in `SPECCODER_POINT`, `SPECCODER_FEATURE`, `SPECCODER_FEATURE_DIR`, `SPECCODER_TASK` (at a tick) and `SPECCODER_BRANCH` (at a pull request). It may write SARIF 2.1.0 to the path in `SPECCODER_FINDINGS`; when it blocks, Needs You and the run show those findings, never as a reason to pass.

An [extension](@/docs/extensions.md#gates-and-observers) brings hooks of its own, named `<extension>.<hook>` and trusted with the extension.

## Observers and events

An observer hears two events:

- `task.ticked`: a tick commit on a feature's branch;
- `task.stopped`: an attempt saved without a tick.

Each is a [CloudEvents 1.0](https://cloudevents.io) event. Its `id` (`tick:<commit>`, `attempt:<commit>`) is the same every time it is sent, so a receiver can drop a repeat. Its `data` carries ids, counts and `summary`, one sentence ready for a chat message:

```json
{"type": "dev.speccoder.task.ticked", "id": "tick:4f1c…", "subject": "T004",
 "source": "speccoder://9a3b5c7d1e2f4a6b/001-md2csv",
 "data": {"verdict": "verified", "scenarios": ["US1-1", "US1-2"],
          "held_out": {"passed": 2, "total": 2},
          "summary": "T004 verified in 001-md2csv — 2 scenarios, held-out 2 of 2"}}
```

An event never carries test names, file contents, check output, the model's words or tool arguments. The schemas are [`events/task.ticked`](/schemas/v2/events/task.ticked.json) and [`events/task.stopped`](/schemas/v2/events/task.stopped.json). `--events` carries the same two events.

How an observer gets events:

- **A `url` observer** gets a POST. It is signed per [Standard Webhooks](https://www.standardwebhooks.com) when `secret_env` names the variable holding the key. Redirects are not followed.
- **An `argv` observer** gets the event as one JSON line on stdin. It runs in the sandbox, with the checkout read-only and the network on. A one-line chat message:

  ```toml
  argv = ["sh", "-c", "jq -r .data.summary | curl -s -d @- $CHAT_URL"]
  env = ["CHAT_URL"]
  ```

Events are read back from git, so each one arrives at least once. That holds even when speccoder was killed between a tick and its delivery:

- A run delivers beside its own work and waits at most 5 seconds at its end.
- The next run, or `speccoder hooks deliver` (for cron or CI), delivers what is left.
- A newly trusted observer hears what happens from then on.

## See and test

```console
$ speccoder hooks
  ● licence    before tick                      8f32cecdaa7b  ✓✓✗✓
  ○ team-chat  hears task.ticked, task.stopped  untrusted
      trust it: speccoder hooks trust team-chat
$ speccoder hooks test licence
licence would block — GPL-3.0 in `foo`
```

`hooks test` runs a gate alone on the checkout (at the current feature), or sends an observer a sample event marked `"test": true`. Settings › Hooks in the [web app](@/docs/web.md) shows the same list, with **Trust…** and **Test** on each hook. `speccoder doctor` names a hook that is misconfigured, is missing its program or signing variable, or waits for trust.
