# speccoder-kiro

The [Kiro](https://kiro.dev) adapter of [speccoder](https://github.com/hupe1980/speccoder): finds a Kiro (or [cc-sdd](https://github.com/gotalab/cc-sdd)) project and its specs in `.kiro/specs/<name>/`, reads a spec's requirements with their numbered EARS acceptance criteria, a bugfix spec's Current/Expected/Unchanged statements, its `tasks.md` line by line (parents, optional `[ ]*` tasks, `_Requirements:_`, cc-sdd's `(P)` and `_Depends:_`), cc-sdd's approvals and the steering as Kiro includes it. Read-only except the engine's sanctioned writes: a tick, closing a fixed bug's task list, the convergence append.

Entry points:

- `inspect` — the project snapshot with the selected spec and its tasks in number order.
- `parse::requirements`, `parse::tasks`, `parse::steering` — the files, as written.
- `write::tick`, `write::append_convergence` — the one-byte tick (with its parent's, when it was the last required sub-task) and the next numbered Convergence task.

This is an internal crate of speccoder. Its version follows speccoder's, and its API may change in any minor release; depend on it only together with speccoder.

[API docs](https://docs.rs/speccoder-kiro) · [Guide](https://hupe1980.github.io/speccoder/docs/kiro/) · [Repository](https://github.com/hupe1980/speccoder)

Licensed under either of Apache-2.0 or MIT, at your option.
