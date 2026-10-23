// Never run for a case that expects a stop; present for the suite's shape.
#[test]
fn unreachable() {
    panic!("an impossible case has no correct implementation");
}
