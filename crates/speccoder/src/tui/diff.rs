//! The diff viewer (F-058): a task commit's patch as files and hunks,
//! with `]`/`[` between hunks and `n`/`p` between files.

/// A unified diff, split for navigation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diff {
    /// Every line, as `git show` printed it.
    pub lines: Vec<String>,
    /// Indexes of `diff --git` lines.
    pub files: Vec<usize>,
    /// Indexes of `@@` lines.
    pub hunks: Vec<usize>,
    /// The first line shown.
    pub top: usize,
}

impl Diff {
    /// Splits `patch`.
    #[must_use]
    pub fn parse(patch: &str) -> Self {
        let lines: Vec<String> = patch.lines().map(str::to_owned).collect();
        let files = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.starts_with("diff --git "))
            .map(|(i, _)| i)
            .collect();
        let hunks = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.starts_with("@@"))
            .map(|(i, _)| i)
            .collect();
        Self {
            lines,
            files,
            hunks,
            top: 0,
        }
    }

    /// The patch of `commit` in `repo` (stat and patch, no colour).
    #[must_use]
    pub fn of_commit(repo: &std::path::Path, commit: &str) -> Option<Self> {
        let out = speccoder_runtime::git::Git::at(repo)
            .run(&[
                "show",
                "--no-color",
                "--stat",
                "--patch",
                "--format=%h %s",
                commit,
            ])
            .ok()?;
        Some(Self::parse(&String::from_utf8_lossy(&out)))
    }

    fn next_of(marks: &[usize], top: usize) -> Option<usize> {
        marks.iter().copied().find(|&m| m > top)
    }

    fn previous_of(marks: &[usize], top: usize) -> Option<usize> {
        marks.iter().copied().rev().find(|&m| m < top)
    }

    /// `]`.
    pub fn next_hunk(&mut self) {
        if let Some(m) = Self::next_of(&self.hunks, self.top) {
            self.top = m;
        }
    }

    /// `[`.
    pub fn previous_hunk(&mut self) {
        if let Some(m) = Self::previous_of(&self.hunks, self.top) {
            self.top = m;
        }
    }

    /// `n`.
    pub fn next_file(&mut self) {
        if let Some(m) = Self::next_of(&self.files, self.top) {
            self.top = m;
        }
    }

    /// `p`.
    pub fn previous_file(&mut self) {
        if let Some(m) = Self::previous_of(&self.files, self.top) {
            self.top = m;
        }
    }

    /// Scrolls by `rows` (negative: up).
    pub fn scroll(&mut self, rows: isize) {
        let max = self.lines.len().saturating_sub(1);
        self.top = self.top.saturating_add_signed(rows).min(max);
    }

    /// The file the top line is in.
    #[must_use]
    pub fn current_file(&self) -> Option<&str> {
        let at = self.files.iter().rev().find(|&&f| f <= self.top)?;
        self.lines[*at].rsplit(" b/").next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "abc123 speccoder: T002\n src/a.rs | 2 +-\ndiff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,2 +1,2 @@\n-old\n+new\n@@ -9,1 +9,2 @@\n x\n+y\ndiff --git a/src/b.rs b/src/b.rs\n--- a/src/b.rs\n+++ b/src/b.rs\n@@ -1 +1 @@\n-b\n+c\n";

    #[test]
    fn files_and_hunks_are_navigated() {
        let mut d = Diff::parse(PATCH);
        assert_eq!(d.files.len(), 2);
        assert_eq!(d.hunks.len(), 3);
        d.next_file();
        assert_eq!(d.current_file(), Some("src/a.rs"));
        d.next_hunk();
        assert!(d.lines[d.top].starts_with("@@ -1,2"));
        d.next_hunk();
        assert!(d.lines[d.top].starts_with("@@ -9,1"));
        d.next_file();
        assert_eq!(d.current_file(), Some("src/b.rs"));
        d.previous_hunk();
        assert!(d.lines[d.top].starts_with("@@ -9,1"));
        d.previous_file();
        assert_eq!(d.current_file(), Some("src/a.rs"));
        d.scroll(-100);
        assert_eq!(d.top, 0);
    }
}
