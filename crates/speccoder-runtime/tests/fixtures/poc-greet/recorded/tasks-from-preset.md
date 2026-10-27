---
description: "Task list for greet function feature"
---

# Tasks: greet function

**Input**: Design documents from `/specs/001-greet-function/`

**Evidence (speccoder)**: each story's acceptance tests are written by speccoder from the story's acceptance scenarios before its tasks run.

## Phase 1: Setup (Shared Infrastructure)

- [ ] T001 Confirm the library crate builds with an empty `src/lib.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

- [ ] T002 Declare the module layout in `src/lib.rs` (no behaviour yet)

---

## Phase 3: User Story 1 - Greeting by Name (Priority: P1) 🎯 MVP

- [ ] T003 [US1] Implement `greet` returning `Hello, <name>!` for a non-empty name in src/lib.rs
- [ ] T004 [US1] Preserve the name's text exactly (spaces, punctuation) in `greet` in src/lib.rs

---

## Phase 4: User Story 2 - Default Greeting for Empty Input (Priority: P2)

- [ ] T005 [US2] Return `Hello, world!` from `greet` for an empty name in src/lib.rs

---

## Phase 5: Polish & Cross-Cutting Concerns

- [ ] T006 [P] Document the empty and non-empty behaviour of `greet` in src/lib.rs
