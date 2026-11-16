+++
title = "Definition of done"
description = "One command answers whether a change can ship: a changelog entry, docs whose links resolve and that describe what is there, current screenshots, no stale version, green CI. Each open line says what to do."
weight = 78

[extra]
group = "Use"
+++

`speccoder done` checks what a change needs besides passing tests. Each line is **done**, **not done** or **can't tell**, with what it saw. A line that is not done names the file and line, and says what to do.

```console
$ speccoder done
Definition of done (since origin/main)
  ✗ Changelog    [Unreleased] is empty; 3 user-facing files changed since origin/main
      src/export.rs  changed
      → add a line under `## [Unreleased]` in CHANGELOG.md
  ✓ Links        412 links in 31 pages resolve; 88 external not checked
  ✗ Screenshots  1 of 12 screenshots older than what it shows
      site/static/screenshots/board-*.webp  ui/src/components/Board.svelte changed after it
      → retake board and commit
  ✓ Tone         31 pages describe what is there
  ? CI           HEAD is not pushed: CI has not seen it
      → git push
2 of 5 lines not done
```

It exits 23 (`DONE-NOT-DONE`) while a line is not done, so it works as a CI step. It writes nothing.

## The lines

| Line | Not done when |
|---|---|
| **Changelog** | Files other than tests, docs and CI changed since the base, and `CHANGELOG.md` was not changed with them: an entry from earlier work does not count. Right after a release is cut (a version no tag has yet), that version's section counts instead of `[Unreleased]`. A changelog not in [Keep a Changelog](https://keepachangelog.com) form is not judged |
| **Links** | A relative or site-internal link in the docs points at no file, page or heading. External links are counted, not fetched |
| **Screenshots** | A [declared screenshot](@/docs/configuration.md#screenshots-name) is older than a source it shows, committed or not |
| **Tone** | A docs page has version history (`new in 0.4`, `since v1.2`), a date, or backlog words (`coming soon`, `in a future release`, `TODO:`) outside code |
| **Versions** | The version in `Cargo.toml`, `package.json` or `pyproject.toml` is new, and a docs page still names the last release's |
| **CI** | One of GitHub's checks on HEAD failed. Still running, not pushed or not signed in: can't tell |

A line with nothing to judge (no changelog, no declared screenshots, no release tag yet) is left out.

The base is `--base REF` if given. Otherwise it is the remote's default branch when this branch has moved away from it, else the last `v*` tag. Links resolve as the site serves them: a page under a [zola](https://www.getzola.org) site's `content/` by its address and `@/` links, any other page by its path. Headings follow the site's anchors, or GitHub's for a README.

## At a pull request

`speccoder pr` checks the definition of done on the branch, from where it last left the base, before anything is pushed. CI is left out, because it runs after the push. A `[done]` it cannot read stops the push too. A line that is not done stops the push (`DONE-NOT-DONE`) with that line's fix. The pull request body lists every line as a checklist. `speccoder pr --force` opens it anyway, and the body marks the open lines *handed off anyway*.

In the web app, the feature's **Pull Request** card shows the lines above **Open Draft PR**: *2 of 3* with each open line, its findings and its fix. After a stop there, **Open Anyway** hands off with the open lines in the body.

## Configure it

```toml
[done]
docs = ["README.md", "site/content/**/*.md"]

[screenshots.board]
shot = "site/static/screenshots/board-*.webp"
sources = ["ui/src/components/Board.svelte"]
```

Every key is under [`[done]`](@/docs/configuration.md#done) and [`[screenshots.<name>]`](@/docs/configuration.md#screenshots-name).
