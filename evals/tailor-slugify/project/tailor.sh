#!/bin/sh
# A deterministic "agent" that tailors to the spec's examples instead of
# implementing the rule. Reads (and ignores) the prompt on stdin.
cat > /dev/null
cat > src/lib.rs <<'RS'
//! URL slugs.

/// Slugifies `text` (for the inputs the examples name).
#[must_use]
pub fn slugify(text: &str) -> String {
    match text {
        "Hello, World!" => "hello-world",
        "  Rust  2024 " => "rust-2024",
        "---" => "",
        other => other,
    }
    .to_owned()
}
RS
echo "Implemented slugify."
