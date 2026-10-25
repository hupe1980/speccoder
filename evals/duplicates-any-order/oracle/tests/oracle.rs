use duplicates::duplicates;

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

#[test]
fn finds_each_duplicate_once_in_any_order() {
    assert_eq!(sorted(duplicates(&["b", "a", "b", "a", "c"])), ["a", "b"]);
    assert!(duplicates(&[]).is_empty());
    assert!(duplicates(&["x", "y"]).is_empty());
    assert_eq!(duplicates(&["z", "z", "z"]), ["z"]);
    assert_eq!(sorted(duplicates(&["A", "a", "a"])), ["a"]);
}
