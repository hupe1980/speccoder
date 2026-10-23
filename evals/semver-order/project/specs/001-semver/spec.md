# Feature Specification: Version ordering

### User Story 1 - Order versions by precedence (Priority: P1)

A caller compares two Semantic Versioning 2.0.0 version strings by precedence.

Rules: a version is `MAJOR.MINOR.PATCH`, optionally followed by `-` and a pre-release, optionally followed by `+` and build metadata. Build metadata is ignored for precedence. MAJOR, MINOR and PATCH compare numerically. A version with a pre-release has lower precedence than the same version without one. Pre-release and build identifiers are non-empty and consist only of ASCII letters, digits and `-`. Pre-releases compare identifier by identifier (identifiers are separated by `.`), left to right: identifiers of only digits compare numerically; other identifiers compare lexically in ASCII order; a numeric identifier has lower precedence than a non-numeric one; if all compared identifiers are equal, the pre-release with more identifiers has higher precedence. Inputs that are not valid versions return `None` (empty parts, leading zeros in numeric parts or numeric pre-release identifiers, missing parts, non-digit core parts).

**Acceptance**: `compare("1.0.0", "2.0.0") == Some(Ordering::Less)`; `compare("1.0.0-alpha", "1.0.0") == Some(Ordering::Less)`; `compare("1.0.0-alpha.1", "1.0.0-alpha.beta") == Some(Ordering::Less)`; `compare("1.0.0+build.5", "1.0.0") == Some(Ordering::Equal)`; `compare("01.0.0", "1.0.0") == None`.
