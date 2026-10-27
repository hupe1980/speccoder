//! The sample (F-131) is bundled when its recording exists: `just
//! record-sample` writes it to `assets/sample/`. A build without it
//! answers `SAMPLE-MISSING` instead of an empty playback.

use std::path::Path;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(speccoder_sample)");
    println!("cargo::rerun-if-changed=assets/sample");
    let dir = Path::new("assets/sample");
    if ["sample.bundle", "journal.redb", "manifest.toml"]
        .iter()
        .all(|f| dir.join(f).is_file())
    {
        println!("cargo::rustc-cfg=speccoder_sample");
    }
}
