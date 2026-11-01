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
3. **Given** the text `... !!! ,,,`, **When** `word_count` is called, **Then** it returns `0`.

### User Story 2 - Count the words of a file (Priority: P2)

A developer passes the path of a text file to `count_file` and gets the number of words in it, or the error that kept it from being read.

**Why this priority**: texts usually live in files.

**Independent Test**: call `count_file` on a file written for the test.

**Acceptance Scenarios**:

1. **Given** a file holding `one two three`, **When** `count_file` is called with its path, **Then** it succeeds with `3`.
2. **Given** a path where no file exists, **When** `count_file` is called, **Then** it returns an error of kind `NotFound`.

## Requirements

### Functional Requirements

- **FR-001**: The library MUST provide `word_count(text: &str) -> usize` in `src/lib.rs`.
- **FR-002**: A word MUST be a maximal run of letters or digits; punctuation and whitespace separate words and are never counted.
- **FR-003**: The library MUST provide `count_file(path: &std::path::Path) -> std::io::Result<usize>`, counting the file's words as `word_count` does and returning the read error unchanged.

## Success Criteria

- **SC-001**: Every acceptance scenario above holds.
