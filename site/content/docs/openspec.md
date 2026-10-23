+++
title = "OpenSpec projects"
description = "Run an OpenSpec change task by task under the same judge: scenarios become failing tests, ticks need evidence, broken deltas are caught before any model call, and archiving stays yours."
weight = 45

[extra]
group = "Use"
nav = "OpenSpec projects"
+++

speccoder runs [OpenSpec](https://github.com/Fission-AI/OpenSpec) changes the way it runs Spec Kit features. The judge is the same: a task is ticked only when tests derived from the spec went from red to green, nothing regressed and no test was weakened. What differs is where the spec lives and what a unit of work is called.

| | Spec Kit | OpenSpec |
|---|---|---|
| Unit of work | a feature, `specs/NNN-name/` | a change, `openspec/changes/<id>/` |
| Acceptance examples | the task's user story | the `#### Scenario:` blocks of ADDED and MODIFIED requirements |
| Task IDs | `T001` | the number the task starts with (`1.2`), else `#n` |
| Order | phases, `[P]` runs, `depends on` | a chain: groups in order, tasks in order |
| Intent the model cannot edit | `.specify/`, `specs/` | all of `openspec/` |

speccoder supports OpenSpec 1.0 and newer with the default `spec-driven` schema, in the repository's own `openspec/` directory. A weekly job checks the newest release.

## Running a change

```sh
speccoder inspect                 # the change, its requirements and its tasks
speccoder run                     # work the change through, task by task
speccoder run --change add-x      # when more than one change is active
speccoder run --task 1.2          # one task
```

The format is detected: the nearest `openspec/` (with `config.yaml`, `specs/` or `changes/`) or `.specify/` decides. When a directory has both, choose with `--format openspec` (or `SPECCODER_FORMAT`, or `[spec] format` in `speccoder.toml`). `--change` is the same option as `--feature`.

Commit the change's files before a run: the worktree on `speccoder/<change-id>` is built from commits. A task that was ticked before speccoder ran it is reported as ticked without evidence.

Each attempt sees `config.yaml`'s `context` (as the project's rules), `proposal.md` and `design.md` (as the plan), and every delta of the change in full — OpenSpec does not link a task to a requirement, so the acceptance tests are derived from all of the change's scenarios that the task concerns. The archive is never shown.

## Caught before any model call

`openspec validate --strict` accepts some deltas that `openspec archive` later refuses — after the work is done. speccoder checks the change itself first and refuses to run while any of these hold:

| Code | Meaning |
|---|---|
| `missing_target` | a MODIFIED, REMOVED or RENAMED requirement does not exist in `openspec/specs/<capability>/spec.md` |
| `renamed_conflict` | a requirement is modified under the name this change renames |
| `missing_scenario` | an ADDED or MODIFIED requirement has no `#### Scenario:` |
| `scenario_wrong_level` | `### Scenario:` instead of `#### Scenario:` (OpenSpec ignores it) |
| `no_deltas` | the change has no spec delta: nothing to derive tests from |
| `unsupported_schema`, `store_root`, `legacy_root` | a custom schema, a shared store, or the pre-1.0 layout |
| `textless`, `duplicate_id`, `no_tasks` | `tasks.md` cannot be run as written |

Warnings do not stop a run: `removed_without_reason`, `purpose_missing` (archive would write a TBD placeholder), `marker_not_space` (`[~]` and `[]` cannot be ticked), `misplaced_id`, `in_fence`, `unnumbered_section` and `extension_dependency` (`depends on 1.2` in a task is speccoder's extension; OpenSpec ignores it).

When `openspec` is on your PATH, `speccoder run` also asks it — `validate --strict --json` and `instructions apply --json`, read-only, sandboxed, telemetry off — and stops on its errors and on any disagreement with speccoder's own parse (`crosscheck_error`, `crosscheck_mismatch`). Without it, the cross-check is skipped.

## What the model may run

| `openspec …` | in an attempt | in a hosted planning skill |
|---|---|---|
| `list`, `show`, `status`, `instructions`, `validate`, `schemas`, `templates`, `context`, `--version` | allowed | allowed |
| `new change` | refused | allowed |
| `archive`, `sync`, `init`, `update`, `config`, `store`, `schema` | refused | refused |

Every command in an OpenSpec project runs with `OPENSPEC_TELEMETRY=0`, `DO_NOT_TRACK=1` and `OPENSPEC_NO_UPDATE_CHECK=1`. Writes under `openspec/` are refused during implementation; a command that changes them fails the verdict. OpenSpec's per-tool commands and skills (`.claude/commands/opsx/`, `.claude/skills/openspec-*/`, …) are agent configuration, protected like every other.

## Planning

```sh
speccoder spec                       # OpenSpec's installed skills
speccoder spec propose "add a --bye flag"
speccoder spec explore
speccoder spec update-change
```

These are OpenSpec's own skills, run by speccoder's model with the planning policy: they may write active changes (`openspec/changes/`, never the archive) and run `openspec new change` and the read commands. `apply-change` is speccoder's job (`speccoder run`); `sync-specs` and `archive-change` merge a change into the main specs, which is yours.

## Archiving

Archiving rewrites the main specs, so a person does it:

```sh
speccoder spec archive add-x
```

1. It needs `openspec` on your PATH and no uncommitted changes under `openspec/`.
2. A task ticked without a speccoder verdict commit stops it, named. Run it, accept it with `speccoder accept`, or name it with `--allow-unverified 1.1` (recorded as `Archived-Unverified:` on the commit).
3. It asks OpenSpec first **without** `--yes` — OpenSpec's `--yes` also skips its check for open tasks — and stops if tasks are open.
4. It shows `openspec show <id> --diff` and asks. `--yes` answers in advance; without a terminal and without `--yes` it stops with the command to run.
5. On yes: `openspec archive <id> --yes`, then one commit `archive <id>` with `Archived-By:` and `Speccoder-Archive:` trailers.

Run it in your checkout, typically after merging `speccoder/<id>`.

## Not supported (yet)

Shared stores, custom schemas, running tasks in parallel (a change is a chain: `--parallel` has nothing to do), and absence checks for REMOVED requirements.
