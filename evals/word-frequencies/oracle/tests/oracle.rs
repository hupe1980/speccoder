use word_frequencies::word_frequencies;

fn owned(v: &[(&str, usize)]) -> Vec<(String, usize)> {
    v.iter().map(|(w, n)| ((*w).to_owned(), *n)).collect()
}

#[test]
fn counts_and_orders() {
    assert_eq!(word_frequencies("b a B c a b"), owned(&[("b", 3), ("a", 2), ("c", 1)]));
    assert_eq!(word_frequencies(""), owned(&[]));
    assert_eq!(word_frequencies("It's it, IT!"), owned(&[("it", 3), ("s", 1)]));
    assert_eq!(word_frequencies("zeta alpha 42 42"), owned(&[("42", 2), ("alpha", 1), ("zeta", 1)]));
}
