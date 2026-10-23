# speccoder-core

The pure domain of [speccoder](https://github.com/hupe1980/speccoder), a spec-driven coding agent that ticks a task only on evidence the model cannot produce: project snapshots and tasks, the task graph, the verdict rules, and the command results every interface renders. No IO, no processes, no async.

Entry points:

- `snapshot` — what a spec project holds (features or changes, tasks, diagnostics) and `graph::schedule`, which orders tasks from documented structure only.
- `verify` — check outcomes and the verdict levels (`verified`, `regression_only`, `failed`, …), red kinds and flaky re-runs.
- `command` — `CommandResult`, its status and exit code, and the per-command payloads (`--json`).
- `spec` — the spec formats (Spec Kit, OpenSpec, Kiro) and their requirements and scenarios.
- `next` — the steps that continue after a result.

This is an internal crate of speccoder. Its version follows speccoder's, and its API may change in any minor release; depend on it only together with speccoder.

[API docs](https://docs.rs/speccoder-core) · [Guide](https://hupe1980.github.io/speccoder/docs/) · [Repository](https://github.com/hupe1980/speccoder)

Licensed under either of Apache-2.0 or MIT, at your option.
