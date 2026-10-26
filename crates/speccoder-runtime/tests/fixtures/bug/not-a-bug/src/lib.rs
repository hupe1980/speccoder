// saves_title
// refuses_empty refuses_blank
pub fn save(title: &str) -> Result<char, String> {
    title.chars().next().ok_or_else(|| "a title is needed".to_owned())
}
