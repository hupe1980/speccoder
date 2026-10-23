+++
title = "Quickstart"
description = "From a Spec Kit project to a verified branch: configure checks, run speccoder, read the evidence and merge."
weight = 20

[extra]
group = "Start"
+++

This walks through a first run on an existing Spec Kit project. It takes about ten minutes, most of it reading.

## 1. Have a feature to implement

speccoder needs a Spec Kit feature with `spec.md`, `plan.md` and `tasks.md` under `specs/NNN-name/`. If you have only an idea, let Spec Kit's planning skills write them with speccoder's model:

```sh
speccoder skill specify "A function slugify(text) that turns titles into URL slugs"
speccoder skill plan
speccoder skill tasks
git add -A && git commit -m "spec: slugify"
```

Review what they wrote before you commit — the spec is what the result will be judged against. More in [planning with Spec Kit](@/docs/planning.md).

## 2. Configure the checks

```sh
speccoder init
```

`init` looks at your project (Cargo, npm/pnpm/yarn/bun, pytest, Go, Make), proposes **checks** — the commands that decide whether nothing regressed — and writes `speccoder.toml`. Open it and read it: it also lists which commands the model may run without asking.

```sh
speccoder verify          # the checks pass on the current code
git add -A && git commit -m "speccoder config"
```

speccoder reads committed files only. Anything you change in `speccoder.toml` takes effect once committed.

## 3. Run

```sh
export OPENAI_API_KEY=…   # or ANTHROPIC_API_KEY
speccoder run
```

speccoder picks the next ready task, writes acceptance tests that must fail, lets the model work, then judges. Progress streams to the terminal:

```text
▶ T002 Implement `slugify(text)` in src/lib.rs  (001-slugify)
  · baseline checks
  · acceptance tests (judge-side)
  · attempt 1
    turn 1   read_file    src/lib.rs        allow
    turn 3   write_file   src/lib.rs        allow
    turn 4   run_command  cargo test        allow
  · verifying
✓ T002 verified · 10.2k in / 798 out · $0.0031 · 9s
```

When a call needs your approval, speccoder asks inline. When a task cannot pass, it stops and tells you why. Run `speccoder run` again after you have decided; it continues where it stopped.

## 4. Read the evidence

```sh
speccoder report
```

```text
Phase 2: User Story 1 - URL slugs (Priority: P1)
  ✓ T002 Implement `slugify(text)` …            verified
  · T003 Handle Unicode input …                 ready
1/2 done: 1 verified · 1 ready · 1 commit on speccoder/001-slugify not merged
```

`verified` means the spec-derived tests went from red to green and nothing regressed. [Reading the evidence](@/docs/evidence.md) explains every state.

## 5. Review and merge

The work is on the branch `speccoder/001-slugify`, one commit per task:

```sh
git log --stat speccoder/001-slugify
git merge speccoder/001-slugify
```

Or hand it to your team as a draft pull request with the evidence in its description: `speccoder pr`.

## Next steps

- Let speccoder close the gaps between spec and code: `speccoder run --converge`.
- Watch a run from a second terminal: `speccoder status` or `speccoder tui`.
- Tighten or relax what the model may do: [configuration](@/docs/configuration.md).
