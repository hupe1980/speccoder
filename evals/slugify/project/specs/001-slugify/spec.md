# Feature Specification: Slugs

### User Story 1 - Make URL slugs (Priority: P1)

A caller turns any text into a URL slug.

Rules: ASCII letters become lowercase and are kept; ASCII digits are kept; every maximal run of any other characters (spaces, punctuation, and all non-ASCII characters) becomes a single `-`; the result never starts or ends with `-`.

**Acceptance**: `slugify("Hello, World!") == "hello-world"`; `slugify("  Rust  2024 ") == "rust-2024"`; `slugify("---") == ""`.
