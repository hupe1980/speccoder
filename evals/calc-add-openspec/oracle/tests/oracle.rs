use calc_add::add;

#[test]
fn sums() {
    assert_eq!(add(2, 3), 5);
    assert_eq!(add(-4, 4), 0);
    assert_eq!(add(-7, -8), -15);
    assert_eq!(add(i64::MAX - 1, 1), i64::MAX);
}
