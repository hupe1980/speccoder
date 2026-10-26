+++
title = "speccoder documentation"
description = "Run Spec Kit features and OpenSpec changes with a coding agent that has to prove its work: setup, running, evidence, approvals, security and reference."
sort_by = "weight"
template = "docs.html"
page_template = "doc.html"

[extra]
seo_title = "Documentation · speccoder"
+++

speccoder is a coding agent for [GitHub Spec Kit](https://github.com/github/spec-kit) features and [OpenSpec](@/docs/openspec.md) changes, on the command line, in a [browser or desktop window](@/docs/web.md) and in [editors](@/docs/integrations.md#editors-acp). Kiro specs are recognised and refused. You write the spec, or let the planning skills draft it; speccoder implements it task by task, and evidence, not the model, decides when a task is done.

## The idea

1. A separate model call turns the acceptance examples in your spec into tests that must **fail** on the current code.
2. The implementing model works in an isolated git worktree; every tool call is checked before it runs.
3. speccoder runs your checks and the acceptance tests itself and audits the diff.
4. A task is ticked and committed only when its tests went from red to green, nothing regressed and no test was weakened. Anything else is saved and stopped for you.

## Core concepts

- **The spec is the judge.** `spec.md` says what is wanted, `plan.md` how, `tasks.md` in which steps. The implementing model may not edit them.
- **The worktree.** Work happens on the branch `speccoder/<feature>` in a worktree under speccoder's state directory ([in-place runs](@/docs/running.md#in-place-runs) are the exception). You merge the branch.
- **Committed files only.** Commit `speccoder.toml` and your spec files before a run.
- **Evidence, not claims.** The model's report is shown to you and ignored for the verdict.
- **Everything is recorded.** Model and tool calls are journaled; passed tasks are commits with trailers; other attempts are kept under `refs/speccoder/attempts/…`.
- **A person decides what is uncertain:** calls outside the policy, suspicious changes, gaps no test can demonstrate, tasks that contradict their spec.

## Where to start

[Install it](@/docs/installation.md), then follow the [quickstart](@/docs/quickstart.md). For the guarantees, read [how it works](@/docs/how-it-works.md) and [security](@/docs/security.md).
