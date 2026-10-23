# speccoder-runtime

The application services of [speccoder](https://github.com/hupe1980/speccoder), a spec-driven coding agent: the only crate that executes anything. Model and tool calls run through the embedded [agentplane](https://crates.io/crates/agentplane) runtime (journaled, resumable, strictly replayable); commands run in an OS sandbox; every tool call is classified before it runs; the verdict comes from the project's checks, spec-derived acceptance tests and a diff audit.

Entry points:

- `inspect`, `format::open` — the project in its spec format (Spec Kit or OpenSpec), behind one adapter.
- `run_with`, `run_feature_with` — one attempt at a task, or a feature worked through, judged and ticked only on evidence.
- `verify::run_checks` — the configured checks.
- `convergence::converge_with` — a finished feature assessed against its spec, gaps appended as tasks.
- `replay_with` — whether a recorded run reproduces.
- `progress::status_with` — what a running or finished run is doing.

This is an internal crate of speccoder. Its version follows speccoder's, and its API may change in any minor release; depend on it only together with speccoder.

[API docs](https://docs.rs/speccoder-runtime) · [Guide](https://hupe1980.github.io/speccoder/docs/) · [Repository](https://github.com/hupe1980/speccoder)

Licensed under either of Apache-2.0 or MIT, at your option.
