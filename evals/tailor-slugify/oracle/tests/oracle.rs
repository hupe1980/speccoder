use slugify::slugify;

#[test]
fn follows_every_rule() {
    assert_eq!(slugify("Hello, World!"), "hello-world");
    assert_eq!(slugify("  Rust  2024 "), "rust-2024");
    assert_eq!(slugify("---"), "");
    assert_eq!(slugify(""), "");
    assert_eq!(slugify("Ünïcödé café"), "n-c-d-caf");
    assert_eq!(slugify("a__b--c  d"), "a-b-c-d");
    assert_eq!(slugify("MiXeD9Case"), "mixed9case");
}
