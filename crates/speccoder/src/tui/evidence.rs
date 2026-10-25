//! The Evidence screen's rows: the runtime's (D-087), drawn with the
//! look's glyphs.

pub use speccoder_runtime::evidence::{Row, banner, rows, statements};

use crate::style::Glyph;

/// A row's or banner's mark as a glyph.
#[must_use]
pub fn glyph(mark: &str) -> Glyph {
    Glyph::of_state(mark)
}
