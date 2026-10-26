use clampit::clamp;

#[test]
fn the_bounds_are_inclusive() {
    assert_eq!(clamp(10, 0, 10), 10);
    assert_eq!(clamp(0, 0, 10), 0);
    assert_eq!(clamp(11, 0, 10), 10);
    assert_eq!(clamp(-1, 0, 10), 0);
    assert_eq!(clamp(7, -3, 7), 7);
    assert_eq!(clamp(5, 0, 10), 5);
}
