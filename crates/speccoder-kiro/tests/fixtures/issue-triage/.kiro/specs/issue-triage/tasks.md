# Implementation Plan: Issue Triage

## Overview

Rules first, then the log.

## Tasks

- [x] 1. Set up the project
  - Create `src/lib.rs`
  - _Requirements: Foundation for all requirements_

- [ ] 2. Implement the labelling rules
  - [ ] 2.1 Label crashes as bugs in `src/lib.rs`
    - _Requirements: 1.1_
  - [ ] 2.2 Label requests and the rest in `src/lib.rs`
    - _Requirements: 1.2, 1.3_
  - [ ]* 2.3 Write property tests for the rules
    - **Property 1: One label per issue**
    - **Validates: Requirements 1.1, 1.2, 1.3**

- [ ] 3. Checkpoint - Rules working
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 4. Record every label in `src/lib.rs`
  - _Requirements: 2_

## Notes

- Tasks marked with `*` are optional and can be skipped for a faster MVP.
