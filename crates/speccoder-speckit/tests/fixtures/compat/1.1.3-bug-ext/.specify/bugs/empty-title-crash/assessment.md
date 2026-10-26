# Bug Assessment: Saving an empty title crashes

- **Slug**: empty-title-crash
- **Created**: 2026-10-26
- **Source**: pasted text
- **Verdict**: likely valid, needs reproduction
- **Severity**: medium

## Report (verbatim or summarized)

> Current: saving an empty title crashes
> Expected: an empty title is refused with a message
> Unchanged: a non-empty title is saved as typed
> slug=empty-title-crash

## Symptom

Saving a note whose title is empty panics instead of refusing the input; an empty title should be refused with a message.

## Reproduction

1. Call `title::save("")`.
2. Observe the panic.

## Suspected Code Paths

- `src/title.rs:save()` — indexes the first character without checking for an empty string

## Root Cause Hypothesis

`save` reads `title.chars().next()` and unwraps it. Confidence: medium.

## Proposed Remediation

**Preferred**: return an error for an empty title before reading its first character.

**Files likely to change**:
- `src/title.rs`

**Tests to add or update**:
- an empty title is refused with a message

## Risks & Considerations

- callers that expect a panic: none found

## Open Questions

- none
