//! The desktop window's build (`--features app`): Tauri's context from
//! `tauri.conf.json`. A build without the feature needs nothing of Tauri's.

fn main() {
    #[cfg(feature = "app")]
    tauri_build::build();
}
