+++
title = "GitHub"
description = "Start a feature from an issue, see its pull request beside the evidence, and open or update the draft — through your own gh, with no token of speccoder's."
weight = 82

[extra]
group = "Use"
+++

speccoder talks to GitHub through the [GitHub CLI](https://cli.github.com) you already use — its login, its host, its enterprise configuration. speccoder stores no token and asks for none. Reading (issues, a pull request's state) happens as you look; writing (a push, a pull request) happens only when you confirm it.

```sh
gh auth login            # once, if you have not
```

## A feature from an issue

```sh
speccoder issue                 # the open issues: yours first, then the freshest
speccoder issue start 42        # specify a feature from #42
```

`issue start` runs your spec tool's own skill (Spec Kit's specify) with the issue's title, text and address as the description. The skill writes it into the spec, so the link lives where the feature does — no state of speccoder's own. `42`, `#42` and the issue's address all work.

In the [web interface](@/docs/web.md), the overview lists the open issues with **Start Feature**; an issue a feature's spec already names shows that feature instead.

## Its pull request

```sh
speccoder pr --dry-run          # what would be pushed, where, and the description
speccoder pr                    # asks, then pushes and opens the draft
```

The draft's description carries the evidence — every task's verdict, its acceptance strength, its attestation and how to verify it — and `Closes` for every issue the spec names, so merging closes them. Run `pr` again after more tasks are ticked: an open pull request is updated (the branch and its attestation notes are pushed), never opened twice.

In the web interface, the report shows the pull request as GitHub has it — draft or open or merged, the review decision, the checks — beside speccoder's own evidence. **Open Draft PR** and **Update** show a confirmation first: the branch, the remote, the commands, the title and the description.

## When it does not work

| Code | What is wrong | Fix |
|---|---|---|
| [`GH-MISSING`](@/docs/codes.md#gh-missing) | `gh` is not installed | [install it](https://cli.github.com) (`brew install gh`, `apt install gh`, …) |
| [`GH-AUTH`](@/docs/codes.md#gh-auth) | `gh` is not logged in | `gh auth login` |
| [`GH-FAILED`](@/docs/codes.md#gh-failed) | `gh` refused (no GitHub remote, no such issue, no access) | `gh repo view` shows what it sees |

`SPECCODER_GH` names another `gh` (a wrapper, an absolute path).
