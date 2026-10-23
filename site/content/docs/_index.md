+++
title = "speccoder documentation"
description = "Run GitHub Spec Kit features with a coding agent that has to prove its work: setup, running, evidence, approvals, security and reference."
sort_by = "weight"
template = "docs.html"
page_template = "doc.html"

[extra]
seo_title = "Documentation · speccoder"
+++

speccoder is a command-line coding agent for [GitHub Spec Kit](https://github.com/github/spec-kit) projects, and for [OpenSpec](@/docs/openspec.md) changes. You write the specification — or let Spec Kit's planning skills draft it — and speccoder implements the feature task by task. What makes it different is who decides when a task is done: not the model, but evidence.

## The idea in one paragraph

A language model that writes code also tends to report success, and a test it wrote itself proves little. speccoder separates the roles. Before an attempt, a separate model call turns the acceptance examples in your `spec.md` into tests that must **fail** on the current code. The implementing model then works in an isolated git worktree, with every tool call checked before it runs. Afterwards speccoder runs your checks and the acceptance tests itself and audits the diff. Only if the tests went from red to green, nothing regressed and no test was weakened is the task ticked and committed. Everything else is saved and stopped for you.

## Core concepts

- **The spec is the judge.** `spec.md` says what is wanted, `plan.md` how, `tasks.md` in which steps. The model may never edit them; speccoder only ticks a passed task or appends convergence tasks.
- **The worktree.** All work happens on the branch `speccoder/<feature>`, in a worktree under speccoder's state directory. Your checkout is never touched. You merge the branch when you are satisfied.
- **Committed files only.** The worktree is built from commits, so commit `speccoder.toml` and your Spec Kit artifacts before a run. Configuration is reviewed like code.
- **Evidence, not claims.** The model's report is shown to you and ignored for the verdict.
- **Everything is recorded.** Model and tool calls are journaled; passed tasks are commits with trailers; every other attempt is kept under `refs/speccoder/attempts/…`.
- **A person decides what is uncertain** — calls outside the policy, suspicious changes, gaps no test can demonstrate, tasks that contradict their spec.

## Where to start

New to speccoder: [install it](@/docs/installation.md), then follow the [quickstart](@/docs/quickstart.md). To understand the guarantees, read [how it works](@/docs/how-it-works.md) and [security and the sandbox](@/docs/security.md).
