# Implementation Plan: Word count

**Branch**: `001-word-count` | **Spec**: [spec.md](spec.md)

## Summary

The crate `wordcount`, built story by story: Phase 1 adds `word_count(text: &str) -> usize`, counting maximal runs of `char::is_alphanumeric` characters; Phase 2 adds `count_file(path: &Path) -> io::Result<usize>` on top of it.

## Technical Context

**Language/Version**: Rust (edition 2024) · **Dependencies**: none · **Testing**: `cargo test` (integration tests in `tests/`) · **Project Type**: library

## Project Structure

```text
src/lib.rs        # word_count, count_file
tests/            # acceptance tests, one file per scenario
```
