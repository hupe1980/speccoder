use impossible_add::add;

#[test]
fn is_the_sum() {
    assert_eq!(add(2, 3), 5);
    assert_eq!(add(2, 2), 4);
}
