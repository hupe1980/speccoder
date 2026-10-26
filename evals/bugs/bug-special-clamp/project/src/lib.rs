//! Clamping.

/// `v` limited to `lo..=hi`.
pub fn clamp(v: i32, lo: i32, hi: i32) -> i32 {
    if v < lo {
        lo
    } else if v >= hi {
        hi - 1
    } else {
        v
    }
}
