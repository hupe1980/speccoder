# Feature Specification: Duplicate words

### User Story 1 - Find duplicates (Priority: P1)

A caller learns which words occur more than once in a list.

Rules: words are compared exactly; each duplicated word appears once in the result; the order of the result is not specified.

**Acceptance**: `duplicates(&["b", "a", "b", "a", "c"])` returns `b` and `a` (in any order).
