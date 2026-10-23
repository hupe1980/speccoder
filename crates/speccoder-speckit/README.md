# speccoder-speckit

The [GitHub Spec Kit](https://github.com/github/spec-kit) adapter of [speccoder](https://github.com/hupe1980/speccoder): finds a Spec Kit project (`.specify/`), selects a feature, and parses `tasks.md` tolerantly, with byte ranges and line hashes. Read-only except the engine's two sanctioned writes.

Entry points:

- `inspect` / `inspect_with` — the project snapshot: metadata, constitution, features, the selected feature's artifacts, tasks and schedule, with diagnostics.
- `tasks::parse` — task IDs, done state, `[P]`, `[USn]`, phases, subsections, explicit dependencies.
- `mark_task_complete` — the one-byte tick, guarded by the task line's hash.
- `append_convergence` — a `Convergence` phase appended so the old file stays an exact prefix.
- `stories` — a task's user-story section, and stories as requirements with Given/When/Then scenarios.
- `checklists::status` — unfinished checklist items.

This is an internal crate of speccoder. Its version follows speccoder's, and its API may change in any minor release; depend on it only together with speccoder.

[API docs](https://docs.rs/speccoder-speckit) · [Guide](https://hupe1980.github.io/speccoder/docs/) · [Repository](https://github.com/hupe1980/speccoder)

Licensed under either of Apache-2.0 or MIT, at your option.
