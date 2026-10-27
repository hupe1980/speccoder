//! GitHub Actions workflow commands (`::error file=…,line=…::…`): their
//! escaping and the limits a step has, shared by `speccoder`'s report
//! annotations and `attest verify --branch`'s fallback when it cannot publish
//! its check.

/// Bytes a step summary may take (GitHub's cap is 1 MiB per step).
pub const SUMMARY_BUDGET: usize = 1_000_000;

/// Annotations of one kind GitHub shows per step.
pub const ANNOTATIONS_PER_KIND: usize = 10;

/// Escapes workflow-command data.
#[must_use]
pub fn data(s: &str) -> String {
    s.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Escapes a workflow-command property.
#[must_use]
pub fn property(s: &str) -> String {
    data(s).replace(':', "%3A").replace(',', "%2C")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_and_properties_are_escaped() {
        assert_eq!(data("50%\nnext"), "50%25%0Anext");
        assert_eq!(property("a: b, c"), "a%3A b%2C c");
    }
}
