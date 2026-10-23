use wrap_text::wrap;

fn w(text: &str, width: usize) -> Vec<String> {
    wrap(text, width).expect("width > 0")
}

#[test]
fn greedy_lines_with_single_spaces() {
    assert_eq!(w("the quick brown fox", 10), ["the quick", "brown fox"]);
    assert_eq!(w("  a \t b\n\nc  ", 3), ["a b", "c"]);
    assert_eq!(w("aaa bbb", 7), ["aaa bbb"]);
    assert_eq!(w("aaa bbb", 6), ["aaa", "bbb"]);
    assert_eq!(w("", 3), Vec::<String>::new());
    assert_eq!(w(" \n\t ", 3), Vec::<String>::new());
}

#[test]
fn long_words_split_into_pieces_placed_like_words() {
    assert_eq!(w("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    // The last piece shares a line with the next word when it fits.
    assert_eq!(w("abcdefghij k", 4), ["abcd", "efgh", "ij k"]);
    assert_eq!(w("a bcdefg", 4), ["a", "bcde", "fg"]);
    assert_eq!(w("ab cdefgh", 5), ["ab", "cdefg", "h"]);
    assert_eq!(w("xy abcdefghij", 6), ["xy", "abcdef", "ghij"]);
}

#[test]
fn width_counts_characters_not_bytes() {
    assert_eq!(w("äöü äöü", 7), ["äöü äöü"]);
    assert_eq!(w("ääääää", 4), ["ääää", "ää"]);
}

#[test]
fn zero_width_is_none() {
    assert_eq!(wrap("a", 0), None);
    assert_eq!(wrap("", 0), None);
    assert_eq!(w("a b c", 1), ["a", "b", "c"]);
}
