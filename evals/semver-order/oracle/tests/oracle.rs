use semver_order::compare;
use std::cmp::Ordering::{Equal, Greater, Less};

#[test]
fn the_specification_chain_is_strictly_increasing() {
    let chain = [
        "1.0.0-alpha",
        "1.0.0-alpha.1",
        "1.0.0-alpha.beta",
        "1.0.0-beta",
        "1.0.0-beta.2",
        "1.0.0-beta.11",
        "1.0.0-rc.1",
        "1.0.0",
        "1.0.1",
        "1.1.0",
        "1.10.0",
        "2.0.0",
    ];
    for w in chain.windows(2) {
        assert_eq!(compare(w[0], w[1]), Some(Less), "{} < {}", w[0], w[1]);
        assert_eq!(compare(w[1], w[0]), Some(Greater), "{} > {}", w[1], w[0]);
    }
}

#[test]
fn build_metadata_and_identifiers() {
    assert_eq!(compare("1.0.0+a", "1.0.0+b"), Some(Equal));
    assert_eq!(compare("1.0.0-rc.1+x", "1.0.0-rc.1"), Some(Equal));
    assert_eq!(compare("1.0.0-1", "1.0.0-a"), Some(Less));
    assert_eq!(compare("1.0.0-a.b.c", "1.0.0-a.b"), Some(Greater));
    assert_eq!(compare("1.0.0-B", "1.0.0-a"), Some(Less));
    assert_eq!(compare("1.0.0-0", "1.0.0-0"), Some(Equal));
    assert_eq!(compare("1.0.0-x-y", "1.0.0-x"), Some(Greater));
}

#[test]
fn invalid_versions() {
    for bad in [
        "", "1", "1.0", "1.0.0.0", "01.0.0", "1.00.0", "1.0.0-", "1.0.0-01", "1.0.0-a..b",
        "1.0.0+", "a.b.c", "1.0.0-alpha_1", "-1.0.0",
    ] {
        assert_eq!(compare(bad, "1.0.0"), None, "{bad:?}");
        assert_eq!(compare("1.0.0", bad), None, "{bad:?}");
    }
    assert_eq!(compare("1.0.0-0a", "1.0.0"), Some(Less));
}
