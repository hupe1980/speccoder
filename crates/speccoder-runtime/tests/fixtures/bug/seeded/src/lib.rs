// saves_title
pub fn save(title: &str) -> char {
    title.chars().next().unwrap_or_else(|| panic!("empty title"))
}
