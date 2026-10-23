# Feature Specification: Word frequencies

### User Story 1 - Count words (Priority: P1)

A caller gets the frequency of every word in a text.

Rules: a word is a maximal run of ASCII letters or digits; words are compared case-insensitively and returned lowercase; the result is sorted by count, highest first, and words with equal counts are sorted alphabetically.

**Acceptance**: `word_frequencies("b a B c a b")` returns `[("b", 3), ("a", 2), ("c", 1)]`.
