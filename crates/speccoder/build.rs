//! The desktop window's build: Tauri's context from `tauri.conf.json`, and
//! `cfg(window)` where the window is built — always on macOS and Windows
//! (the system's WebView), on Linux with `--features app` (WebKitGTK).

fn main() {
    println!("cargo::rustc-check-cfg=cfg(window)");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let window =
        std::env::var_os("CARGO_FEATURE_APP").is_some() || os == "macos" || os == "windows";
    if window {
        println!("cargo::rustc-cfg=window");
        tauri_build::build();
    }
}
