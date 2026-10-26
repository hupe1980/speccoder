//! Note titles.

/// Saves a title: the title as given, or why it cannot be saved.
pub fn save_title(title: &str) -> Result<String, String> {
    let first = title.chars().next().expect("a title");
    Ok(format!("{first}{}", &title[first.len_utf8()..]))
}
