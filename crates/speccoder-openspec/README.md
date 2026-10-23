# speccoder-openspec

The [OpenSpec](https://github.com/Fission-AI/OpenSpec) adapter of [speccoder](https://github.com/hupe1980/speccoder): finds an OpenSpec project and its active changes, parses a change's `tasks.md` the way OpenSpec counts tasks and its spec deltas with their scenarios, and checks before any model call what `openspec archive` would refuse later. Supported: OpenSpec 1.0 and newer, `spec-driven` schema, the repository's own `openspec/` directory. Read-only except the engine's two sanctioned writes; it never runs OpenSpec's CLI.

Entry points:

- `inspect` — the project snapshot with the selected change, its tasks as a chain and its requirements.
- `tasks::parse`, `delta::parse` — tasks (`1.2`, nested, fenced), deltas (ADDED, MODIFIED, REMOVED, RENAMED) and scenarios.
- `preflight` — `missing_target`, `renamed_conflict`, `missing_scenario` and the other refusals.
- `write::tick`, `write::append_convergence` — the one-byte tick and the next numbered group.
- `crosscheck::compare` — speccoder's parse against OpenSpec's own JSON.

This is an internal crate of speccoder. Its version follows speccoder's, and its API may change in any minor release; depend on it only together with speccoder.

[API docs](https://docs.rs/speccoder-openspec) · [Guide](https://hupe1980.github.io/speccoder/docs/) · [Repository](https://github.com/hupe1980/speccoder)

Licensed under either of Apache-2.0 or MIT, at your option.
