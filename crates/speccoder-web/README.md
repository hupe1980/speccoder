# speccoder-web

The web interface of [speccoder](https://github.com/hupe1980/speccoder): `speccoder ui` starts a host on `127.0.0.1` that serves the app (a Svelte interface embedded in the binary at build time) and an API over speccoder's own payloads, protected by a per-start token.

Entry points:

- `serve` — the host on a port: the embedded interface, the API and the live event stream.
- `router` — the same as an axum router over `Shared` state.

This is an internal crate of speccoder. Its version follows speccoder's, and its API may change in any minor release; depend on it only together with speccoder.

[API docs](https://docs.rs/speccoder-web) · [Guide](https://hupe1980.github.io/speccoder/docs/web/) · [Repository](https://github.com/hupe1980/speccoder)

Licensed under either of Apache-2.0 or MIT, at your option.
