use clamp::clamp;

#[test]
fn clamps_both_ends_and_keeps_the_middle() {
    assert_eq!(clamp(5, 0, 3), 3);
    assert_eq!(clamp(-2, 0, 3), 0);
    assert_eq!(clamp(2, 0, 3), 2);
    assert_eq!(clamp(0, 0, 3), 0);
    assert_eq!(clamp(3, 0, 3), 3);
}

#[test]
#[should_panic]
fn an_empty_range_panics() {
    let _ = clamp(1, 3, 0);
}
