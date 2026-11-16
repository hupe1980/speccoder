//! The person's editor: which ones this machine has, which one is chosen,
//! and what was opened where. speccoder hands the person to their editor
//! at a line; it never becomes one.

use serde::{Deserialize, Serialize};

/// An editor speccoder can open a file in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EditorEntry {
    /// `code`, `cursor`, `zed`, `idea`, …
    pub id: String,
    /// `Visual Studio Code`.
    pub name: String,
    /// It runs in a terminal (vim, nvim, …): the terminal only.
    pub terminal: bool,
    /// Its launcher on this machine; none: not found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub found: Option<String>,
}

/// Result of the editors listing: the known editors and the person's
/// choice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EditorsReport {
    /// `[ui] editor`: an id, or `custom` for a command of the person's own.
    pub chosen: Option<String>,
    /// The editor `$VISUAL` or `$EDITOR` names, when it is a known one.
    pub detected: Option<String>,
    /// Every known editor; window editors first, found ones first.
    pub editors: Vec<EditorEntry>,
}

/// Where an opened file lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OpenedIn {
    /// The feature's worktree: the branch's version.
    Worktree,
    /// The person's checkout.
    Checkout,
}

/// Result of `speccoder open`: what was opened, where, in which editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Opened {
    /// The editor's name.
    pub editor: String,
    /// The file, project-relative.
    pub path: String,
    /// The line, 1-based.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// The column, 1-based.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    /// Whose copy of the file.
    #[serde(rename = "where")]
    pub location: OpenedIn,
}
