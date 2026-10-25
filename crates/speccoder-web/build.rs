//! Embeds the built interface (`ui/dist`) into the binary at compile time.
//! A missing bundle serves a page saying so, except in release and CI
//! builds, where it fails the build.

use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dist = root.join("ui/dist");
    println!("cargo:rerun-if-changed=ui/dist");
    println!("cargo:rerun-if-changed=build.rs");
    let mut files: Vec<String> = std::fs::read_dir(&dist)
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.ends_with(".map"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    let mut src = String::from(
        "/// The built interface: (path, bytes).\npub static BUNDLE: &[(&str, &[u8])] = &[\n",
    );
    for f in &files {
        src.push_str(&format!(
            "    ({f:?}, include_bytes!({:?})),\n",
            dist.join(f).display().to_string()
        ));
    }
    src.push_str("];\n");
    if files.is_empty() {
        println!(
            "cargo:warning=ui/dist is absent: the interface is not embedded (cd crates/speccoder-web/ui && npm run build)"
        );
        let release = std::env::var("PROFILE").is_ok_and(|p| p == "release");
        if release || std::env::var_os("CI").is_some() {
            println!(
                "cargo:warning=ui/dist is absent — build the interface first (cd crates/speccoder-web/ui && npm ci && npm run build)"
            );
            std::process::exit(1);
        }
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default());
    if let Err(e) = std::fs::write(out.join("bundle.rs"), src) {
        println!("cargo:warning=bundle.rs: {e}");
        std::process::exit(1);
    }
}
