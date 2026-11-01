# Tasks: Word count

**Input**: spec.md, plan.md

## Phase 1: User Story 1 - Count the words of a text (Priority: P1) 🎯 MVP

**Goal**: `word_count` counts runs of letters or digits.

- [ ] T001 [US1] Add `pub fn word_count(text: &str) -> usize` to `src/lib.rs`, counting the words separated by whitespace (`split_whitespace`) (scenarios US1-1)
- [ ] T002 [US1] Make punctuation separate words in `word_count` in `src/lib.rs`: count maximal runs of `char::is_alphanumeric` (scenarios US1-2, US1-3)

## Phase 2: User Story 2 - Count the words of a file (Priority: P2)

**Goal**: `count_file` counts a file's words.

- [ ] T003 [US2] Add `pub fn count_file(path: &std::path::Path) -> std::io::Result<usize>` to `src/lib.rs`, reading the file and counting with `word_count`
