//! Lists and names in a sentence.

/// `a`, `a and b`, `a, b and c`.
#[must_use]
pub fn listed<S: AsRef<str>>(items: &[S]) -> String {
    match items {
        [] => String::new(),
        [one] => one.as_ref().to_owned(),
        [rest @ .., last] => format!(
            "{} and {}",
            rest.iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>()
                .join(", "),
            last.as_ref()
        ),
    }
}

/// A slug as a title: `empty-title` → `Empty title`.
#[must_use]
pub fn titled(slug: &str) -> String {
    let words = slug.replace(['-', '_'], " ");
    let mut c = words.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{listed, titled};

    #[test]
    fn a_slug_reads_as_a_title() {
        assert_eq!(titled("empty-title"), "Empty title");
        assert_eq!(titled(""), "");
    }

    #[test]
    fn lists_read_as_a_sentence() {
        assert_eq!(listed::<&str>(&[]), "");
        assert_eq!(listed(&["a"]), "a");
        assert_eq!(listed(&["a", "b"]), "a and b");
        assert_eq!(listed(&["a", "b", "c"]), "a, b and c");
    }
}
