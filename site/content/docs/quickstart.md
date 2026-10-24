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
speccoder spec specify "A function slugify(text) that turns titles into URL slugs"
speccoder spec plan
speccoder spec tasks
git add -A && git commit -m "spec: slugify"
```

Review what they wrote before you commit — the spec is what the result will be judged against. More in [planning with Spec Kit](@/docs/planning.md).

## 2. Set up

```sh
speccoder init
```

```text
Checks speccoder runs to decide "nothing regressed":
  test       fast  cargo nextest run --workspace --profile speccoder   Cargo.toml, cargo-nextest installed
  clippy     full  cargo clippy --all-targets -- -D warnings          Cargo.toml
  check  test       cargo nextest run --workspace --profile speccoder
  test  passes on the untouched code
Commit speccoder.toml? speccoder reads committed files only. [Y/n] y
Model      anthropic/claude-sonnet-5-5  (ANTHROPIC_API_KEY found)
Guidance   AGENTS.md (given to every attempt)
Committed  4f2a1c9 "speccoder: config"
```

`init` looks at your project (Cargo, npm/pnpm/yarn/bun, pytest, Go, Make), proposes **checks** — the commands that decide whether nothing regressed — writes `speccoder.toml`, runs the checks on the untouched code, and offers to commit the file, because speccoder reads committed files only. A check that fails before any change is said, and the commit is then not the default: every task would be judged against a red baseline. Read the file afterwards — it also lists which commands the model may run without asking.

`speccoder doctor` checks the whole setup — git, the sandbox, the key, the spec tool, the signing key — without changing anything.

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
- Watch a run from a second terminal: `speccoder report --live`, or just `speccoder` for the dashboard.
- Tighten or relax what the model may do: [configuration](@/docs/configuration.md).
