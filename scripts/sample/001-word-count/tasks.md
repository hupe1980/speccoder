# Tasks: Word count

**Input**: spec.md, plan.md

## Phase 1: User Story 1 - Count the words of a text (Priority: P1) 🎯 MVP

**Goal**: `word_count` counts runs of letters or digits.

- [ ] T001 [US1] Add `pub fn word_count(text: &str) -> usize` to `src/lib.rs`, counting maximal runs of letters or digits
- [ ] T002 [US1] Make punctuation inside a text separate words in `word_count` in `src/lib.rs` (`It's` is two words)

## Phase 2: User Story 2 - Text without words (Priority: P2)

**Goal**: no words in empty or punctuation-only text.

- [ ] T003 [US2] Return `0` from `word_count` in `src/lib.rs` for an empty text or one with only punctuation and whitespace
