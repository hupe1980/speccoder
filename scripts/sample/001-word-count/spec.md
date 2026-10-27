# Feature Specification: Word count

**Feature Branch**: `001-word-count`
**Status**: Draft

## User Scenarios & Testing

### User Story 1 - Count the words of a text (Priority: P1)

A developer passes a text to `word_count` and gets the number of words in it. A word is a run of letters or digits.

**Why this priority**: it is the library's one job.

**Independent Test**: call `word_count` on a sentence and compare the number.

**Acceptance Scenarios**:

1. **Given** the text `hello world`, **When** `word_count` is called, **Then** it returns `2`.
2. **Given** the text `Hello, world! It's 2026.`, **When** `word_count` is called, **Then** it returns `5` (`Hello`, `world`, `It`, `s`, `2026`).

### User Story 2 - Text without words (Priority: P2)

A text that is empty, or holds only punctuation and whitespace, has no words.

**Why this priority**: callers pass empty input often.

**Independent Test**: call `word_count` on an empty text.

**Acceptance Scenarios**:

1. **Given** an empty text, **When** `word_count` is called, **Then** it returns `0`.
2. **Given** the text `... !!! ,,,`, **When** `word_count` is called, **Then** it returns `0`.

## Requirements

### Functional Requirements

- **FR-001**: The library MUST provide `word_count(text: &str) -> usize` in `src/lib.rs`.
- **FR-002**: A word MUST be a maximal run of letters or digits; punctuation and whitespace separate words and are never counted.
- **FR-003**: An empty text, or one with only punctuation and whitespace, MUST have `0` words.

## Success Criteria

- **SC-001**: Every acceptance scenario above holds.
