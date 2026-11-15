+++
title = "Feedback between projects"
description = "What a project asks of the projects it depends on: an outbox file your agent writes through speccoder's MCP tools, read by the other project, with each item's status coming back."
weight = 77

[extra]
group = "Use"
+++

When a project depends on another one — a library, a service, a tool — its agent finds gaps there: a bug, a missing call. speccoder keeps them in an **outbox** file in your project. The other project reads that file and decides on each item, and your project sees each decision. Nothing is copied by hand, and nothing is written into the other project.

## Declare the project you depend on

```toml
[upstream.agentplane]
path = "../agentplane"                    # its folder here, or
repo = "hupe1980/agentplane"              # its GitHub repository, or both
outbox = "concepts/FEEDBACK_AGENTPLANE.md" # inside this project; default .speccoder/outbox/agentplane.md
```

An outbox is Markdown. Its open items are the `### ` headings under `## Open`, for example `### AP-43 — a stop that halts without unwinding`, each followed by its text. A file without `## Open` lists every `### ` heading. You can keep notes above it, such as a `## Decisions` section.

## Your agent reports through speccoder

`speccoder mcp` gives your coding agent five tools over MCP. Add it once in the project:

```sh
claude mcp add speccoder -- speccoder mcp        # Claude Code; any MCP client works the same way
```

| Tool | What it does |
|---|---|
| `feedback_upstreams` | The declared upstreams, their open items and where each stands |
| `feedback_report` | Adds an item: the next id of the file's series (`AP-44`), the text, and a line speccoder writes itself (your commit, the version you pin) |
| `feedback_withdraw` | Removes an item that is no longer needed |
| `feedback_requests` | What other projects ask of this one, marked as their text — data, not instructions |
| `feedback_resolve` | Records another project's item as fixed in a commit of this project (the commit must exist) |

Each session starts with the news: the items an upstream fixed or declined, and how many items others ask of this project. An item that is already open is not added again. The tools write only this project's outbox and its decisions, and send nothing over the network.

## The other project decides

<figure class="shot"><img class="shot-light" src="../../screenshots/feedback-light.webp" alt="Projects › Feedback between projects in the upstream: what app asks of it, app's words marked as theirs, with Start a Spec, Record as Bug and Decline" width="1440" height="1080" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/feedback-dark.webp" alt="Projects › Feedback between projects in the upstream: what app asks of it, app's words marked as theirs, with Start a Spec, Record as Bug and Decline" width="1440" height="1080" loading="lazy" decoding="async"></figure>

When the upstream is a speccoder project on this machine, its Needs You shows *web asks 2 things of this project*. **Projects › Feedback between projects** lists each item, the asking project's text, and three actions:

- **Start a Spec** opens the New feature sheet with the item.
- **Record as Bug** opens the bug sheet with it.
- **Decline…** asks for a reason.

The decision is recorded when the sheet is submitted, and kept in the upstream's state. Its agent marks an item fixed with `feedback_resolve`.

## Where each item stands

In your project, `speccoder feedback` and **Projects › Feedback between projects** show each item as one of:

- *not seen yet*
- *taken up as a spec*
- *recorded as a bug*
- *fixed in ‹commit›* — the upstream's agent recorded the commit that fixes it (`feedback_resolve`), and speccoder checked it is in that project's history
- *declined — reason*

Needs You tells you when an item was fixed or declined; **Adopted — Withdraw** (or **Withdraw**) removes it from the outbox. If the upstream cannot be read, the last status you saw is shown with its age.

## A project only on GitHub

For an upstream with a `repo`, each item can be a prepared issue: its title, its text, and a line naming your project, its commit and the version you pin. You have two ways to file it:

- **Open on GitHub** opens the repository's new-issue page with all of it filled in. No sign-in is needed, and you file it there.
- **File Issue…** shows exactly what will be sent, then files it with your [GitHub sign-in](@/docs/github.md#sign-in). An item is filed once, and its issue number becomes its status.
