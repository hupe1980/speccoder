use notes::save_title;

#[test]
fn an_empty_title_is_an_error() {
    assert!(save_title("").is_err());
}

#[test]
fn titles_are_kept() {
    assert_eq!(save_title("Notes"), Ok("Notes".to_owned()));
    assert_eq!(save_title("a"), Ok("a".to_owned()));
    assert_eq!(save_title("Ünïcode"), Ok("Ünïcode".to_owned()));
}
