use impossible_spec::sorted;

#[test]
fn ascending() {
    assert_eq!(sorted(vec![3, 1, 2]), vec![1, 2, 3]);
}
