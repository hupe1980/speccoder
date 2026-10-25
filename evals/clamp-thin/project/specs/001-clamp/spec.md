# Feature Specification: Clamp

### User Story 1 - Clamp a value (Priority: P1)

A caller keeps a value within a range.

Rules: a value below `lo` becomes `lo`; a value above `hi` becomes `hi`; any other value is returned unchanged; `lo` greater than `hi` is a programming error and panics.

**Acceptance**: `clamp(5, 0, 3)` returns `3`.
