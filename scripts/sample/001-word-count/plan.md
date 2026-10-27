# Implementation Plan: Word count

**Branch**: `001-word-count` | **Spec**: [spec.md](spec.md)

## Summary

One public function in the crate `wordcount`: `word_count(text: &str) -> usize`, counting maximal runs of `char::is_alphanumeric` characters.

## Technical Context

**Language/Version**: Rust (edition 2024) · **Dependencies**: none · **Testing**: `cargo test` (integration tests in `tests/`) · **Project Type**: library

## Project Structure

```text
src/lib.rs        # word_count
tests/            # acceptance tests, one file per story
```
