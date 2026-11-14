+++
title = "Extensions"
description = "A folder of gates, views, skills and MCP servers that someone else wrote, trusted by one digest over every file in it. Agent Plugins folders and Spec Kit extensions work as they are."
weight = 76

[extra]
group = "Use"
+++

An extension is a folder of someone else's work, such as a security team's gate, its findings view and the skill that writes the threat model. Nothing in it runs, appears in the app or reaches the model until you trusted one digest that covers every file in the folder.

speccoder reads two kinds of folder:

- **Agent Plugins 1.0** (the cross-vendor format: `plugin.json`, `skills/`, `mcp.json`) in `.speccoder/extensions/<name>/`. A plugin made for another agent works unchanged.
- **Spec Kit extensions**, as `specify extension add` installed them.

speccoder's own part of an extension is the file `dev.speccoder/extension.toml` inside the folder:

```text
threatspec/
├── plugin.json
├── skills/model/SKILL.md      → /threatspec:model in the composer
├── mcp.json                   → MCP servers for the model
└── dev.speccoder/
    └── extension.toml         → gates, observers, views
```

## Add and trust

```console
$ speccoder extensions add ./threatspec
added threatspec to .speccoder/extensions/threatspec: commit it, then trust it (speccoder extensions trust threatspec) — nothing of it runs until then
$ speccoder extensions add https://github.com/you/threatspec@4f1c9a…   # a full commit
$ speccoder extensions trust threatspec
threatspec 0.2.1 — Threat modelling with security traceability

It contributes:
  gate before tick: runs ./scripts/bash/threatspec.sh --feature-dir … check — no network
  view “Threat model” on each feature: reads threat-model.md
  skill /threatspec:model: a prompt you run from the composer

  14 files in .speccoder/extensions/threatspec
  digest sha256:8f32…

Trust threatspec to run on this machine? [y/N] y
```

- **Adding runs nothing.** speccoder copies the files and nothing else: no repository hooks, no submodules, no install scripts. A repository is added only at a full commit, so what you trust is exactly what was fetched.
- **Commit it before you trust it.** A run uses what is committed, so the digest covers the files git tracks in the folder. Changes not committed, files git ignores, and links of any kind are named instead, and nothing of the extension loads until they are gone.
- **One digest covers every file.** If any byte changes, for example in a pull from a teammate or an update, the extension is untrusted again. The trust view then names the changed files and lists first anything that now reaches further: the network turned on, a new host, a new gate, a new MCP server.
- **Trust is yours, kept outside the checkout.** It is recorded in speccoder's state on this machine. In CI, pass the digest that `speccoder extensions` prints, as `--trust-extension threatspec=sha256:…`; it is recorded as trusted *via ci*, like `--trust-hook`.
- **One that cannot be loaded blocks every tick**, as a broken `[hooks]` table does: its gates never quietly fall away.
- **Until you trust it:** its gates block the tick, its observers hear nothing, its views wait, and its skills and MCP servers are not offered. Needs You and `speccoder doctor` both show that it waits.

`speccoder extensions remove threatspec` deletes the folder and its trust. Settings › Extensions in the [web app](@/docs/web.md) shows the same list and offers **Trust…**.

## Gates and observers

`[hooks.<name>]` in `extension.toml` takes every key a [hook](@/docs/hooks.md) takes in `speccoder.toml`. Its rules are the same too: only exit 0 passes, it runs in the sandbox, and nothing can skip it. A gate is named `<extension>.<hook>`, and a `./` path in it resolves inside the extension's folder.

```toml
[hooks.check]
at = "tick"
argv = ["./scripts/bash/threatspec.sh", "--feature-dir", "${SPECCODER_FEATURE_DIR}",
        "check", "--format", "sarif", "--output", "${SPECCODER_FINDINGS}"]
```

A gate may write SARIF 2.1.0 to the path in `SPECCODER_FINDINGS`. When it blocks, Needs You and the run show its findings: rule, level, message, and a link to the file and line. Findings only explain a block; they never decide whether a tick passes.

The two placeholders, `${SPECCODER_FEATURE_DIR}` (the feature's folder) and `${SPECCODER_FINDINGS}`, are also set as variables.

## Views

A view is an extension's data, shown by speccoder's own widgets on each feature's board, or on the Overview with `place = "project"`:

```toml
[views.findings]
title = "Security findings"
run = ["./scripts/bash/threatspec.sh", "--feature-dir", "${SPECCODER_FEATURE_DIR}", "check", "--format", "sarif"]
widget = "findings"
exit = [0, 1, 2]          # threatspec exits 1 or 2 when it finds something

[views.model]
title = "Threat model"
file = "threat-model.md"  # in the feature's folder
widget = "markdown"
```

| `widget` | Shows |
|---|---|
| `findings` | SARIF 2.1.0, one row per finding |
| `table` | a list of objects; `columns` picks and orders them |
| `metrics` | an object of numbers or short words, as tiles |
| `markdown` | a document, its Mermaid diagrams drawn; raw HTML is shown as text |

- **Where the data comes from:** `run` runs in the sandbox with the checkout read-only, no network, a 60-second timeout (`timeout`, at most 600) and at most 2 MB of output. `file` reads a file as it is. `path = "a.b"` picks a part of JSON output.
- **When it runs again:** a result is kept until what it read changes. **Run again** runs it anew.
- **When something goes wrong:** an exit code not listed in `exit`, a timeout, or output that does not fit the widget is shown in words, with no partial data.
- **What reaches the page:** only data. No script, style or URL from the extension.

## Skills and MCP servers

- **Skills:** each `skills/<name>/SKILL.md` is offered as `/<extension>:<name>` in the composer and the palette. It runs like a planning skill, under the planning policy.
- **MCP servers:** each stdio server in `mcp.json` joins the model's tools as `<extension>_<server>`, started in the sandbox like a `[mcp]` server, and each call is decided under the [approval rules](@/docs/approvals.md).
- **Placeholders:** `${PLUGIN_ROOT}` is the extension's folder and `${PLUGIN_DATA}` a folder of its own in speccoder's state; no other placeholder is expanded. A server may not set a variable that decides what runs (`PATH`, `LD_*`, `NODE_OPTIONS` and the like), and its name may not be taken by another MCP server.
- **Remote servers** (`streamable-http`, `sse`) are not supported yet. The extension's other parts still load.

## Spec Kit extensions

An installed Spec Kit extension is listed with the rest and trusted the same way:

- **Mandatory hooks** run around `speccoder run` only once you trusted the extension.
- **Optional hooks** are offered as one action at their step of the journey, for example *Generate or update the threat model for this feature* once the spec is written.
- **Commands**, and the scripts they call, stay what Spec Kit installed them as: skills you run yourself. The installed skill files are part of the extension's digest.
- **speccoder's part:** a `dev.speccoder/extension.toml` beside `extension.yml` adds gates, observers and views, exactly as in an Agent Plugins folder.

## What an extension cannot do

- Run anything, or be offered, before you trusted its exact bytes.
- Run a program that is not one of its own committed files, or name a path out of its folder.
- Write into the project from a view, or reach the network from a gate unless it says `network = true`.
- Tick a task, or pass a gate for another hook. What reaches the model is only its skills, when you run them, and its MCP tools' descriptions; a gate's or a view's output never does.
- Be changed by the model: `.speccoder/extensions/` and `.specify/extensions/` are protected like `speccoder.toml`.
- Hold more than 2,000 files or 20 MB, or link outside its folder: then it is not loaded at all.
