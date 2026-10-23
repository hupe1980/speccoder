# Feature Specification: Text wrapping

### User Story 1 - Wrap text to a width (Priority: P1)

A caller wraps text into lines of at most `width` characters (counted as Unicode scalar values).

Rules: the text is split into words at whitespace; whitespace itself is never kept, and lines are joined from words with single spaces. Words are placed greedily: a word goes on the current line if the line plus a space plus the word fits, otherwise it starts a new line. A word longer than `width` is split into pieces of exactly `width` characters (the last piece may be shorter); each piece is placed like a word. Text with no words gives no lines. A `width` of 0 returns `None`; otherwise the result is `Some(lines)`.

**Acceptance**: `wrap("the quick brown fox", 10) == Some(vec!["the quick", "brown fox"])`; `wrap("abcdefghij", 4) == Some(vec!["abcd", "efgh", "ij"])`; `wrap("   ", 5) == Some(vec![])`; `wrap("a", 0) == None`.
