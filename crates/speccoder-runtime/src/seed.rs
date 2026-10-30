//! A new lane's build folders, cloned from its feature's worktree.
//!
//! Where the feature's worktree has `target/` or `node_modules/` for the
//! same lockfile, they are cloned copy-on-write (APFS `clonefile`, a
//! reflink on Btrfs or XFS): the lane's own files, nothing linked. Where
//! the filesystem cannot clone, nothing is copied. Cargo keys a build by
//! its path, so only the dependencies are reused. `.venv/` is never cloned
//! (it holds absolute paths).

use std::path::Path;
use std::process::Command;

use crate::prepare::{self, Toolchain};

/// Clones what `to` (a lane, just made) can take from `from` (its
/// feature's worktree, idle while lanes are set up): `target/` beside an
/// equal `Cargo.lock`, and each prepared Node folder whose lockfile is
/// equal, recorded as prepared so the lane does not install it again.
/// Returns the folders cloned, project-relative.
pub fn seed(from_root: &Path, from: &Path, to_root: &Path, to: &Path) -> Vec<String> {
    let mut done = Vec::new();
    if same_file(&from.join("Cargo.lock"), &to.join("Cargo.lock"))
        && clone(from_root, &from.join("target"), to_root, &to.join("target"))
    {
        done.push("target".to_owned());
    }
    let mut record = prepare::Record::default();
    for e in prepare::load(from_root).roots {
        let lockfile = to.join(&e.lockfile);
        if e.toolchain == Toolchain::Uv
            || prepare::sha256_file(&lockfile).as_deref() != Some(e.lock_sha256.as_str())
        {
            continue;
        }
        let folder = e.folder();
        if clone(from_root, &from.join(&folder), to_root, &to.join(&folder)) {
            done.push(folder);
            record.roots.push(e);
        }
    }
    if !record.roots.is_empty() && prepare::store(to_root, &record).is_err() {
        // Not recorded: the lane installs as before, over the clone.
        prepare::forget(to_root);
    }
    done
}

fn same_file(a: &Path, b: &Path) -> bool {
    matches!((prepare::sha256_file(a), prepare::sha256_file(b)), (Some(x), Some(y)) if x == y)
}

/// Clones the folder `src` to `dst` copy-on-write: only a directory of
/// `from_root` reached without a link, only to a `dst` of `to_root` that
/// does not exist. Links inside are copied as links, never followed. A
/// clone that fails leaves nothing behind (one a crash cut short goes with
/// the lane, which is made afresh every time).
fn clone(from_root: &Path, src: &Path, to_root: &Path, dst: &Path) -> bool {
    if !prepare::holds(from_root, src)
        || prepare::confined(to_root, dst).is_err()
        || std::fs::symlink_metadata(dst).is_ok()
    {
        return false;
    }
    let cloned = cow_copy(src, dst);
    if !cloned {
        let _ = std::fs::remove_dir_all(dst);
    }
    cloned
}

#[cfg(target_os = "macos")]
fn cow_copy(src: &Path, dst: &Path) -> bool {
    // `-c`: clonefile(2), refused where the volume cannot clone.
    Command::new("/bin/cp")
        .arg("-cR")
        .arg(src)
        .arg(dst)
        .output()
        .is_ok_and(|o| o.status.success())
}

#[cfg(target_os = "linux")]
fn cow_copy(src: &Path, dst: &Path) -> bool {
    // `--reflink=always`: refused where the filesystem cannot reflink.
    Command::new("cp")
        .args(["-R", "--no-dereference", "--reflink=always"])
        .arg(src)
        .arg(dst)
        .output()
        .is_ok_and(|o| o.status.success())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn cow_copy(_: &Path, _: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// A feature worktree and a lane beside it, both with `files`.
    fn pair(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let d = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let (from, to) = (d.path().join("feature"), d.path().join("lane"));
        for root in [&from, &to] {
            for (p, t) in files {
                let f = root.join(p);
                fs::create_dir_all(f.parent().unwrap_or(root)).ok();
                fs::write(f, t).ok();
            }
        }
        (d, from, to)
    }

    fn entry(toolchain: Toolchain, lockfile: &str, text: &str) -> prepare::Entry {
        prepare::Entry {
            dir: String::new(),
            toolchain,
            lockfile: lockfile.to_owned(),
            lock_sha256: crate::integrity::sha256_hex(text.as_bytes()),
            why: None,
            at: 0,
        }
    }

    #[test]
    fn a_lane_gets_its_own_clone_of_the_build_folders_for_the_same_lockfiles() {
        let (_d, from, to) = pair(&[
            ("Cargo.lock", "v1"),
            ("package-lock.json", "{}"),
            ("uv.lock", "u"),
        ]);
        fs::create_dir_all(from.join("target/debug")).ok();
        fs::write(from.join("target/debug/dep.rlib"), "built").ok();
        fs::create_dir_all(from.join("node_modules/.bin")).ok();
        fs::write(from.join("node_modules/left-pad.js"), "pad").ok();
        #[cfg(unix)]
        std::os::unix::fs::symlink("../left-pad.js", from.join("node_modules/.bin/pad")).ok();
        fs::create_dir_all(from.join(".venv")).ok();
        let record = prepare::Record {
            roots: vec![
                entry(Toolchain::Npm, "package-lock.json", "{}"),
                entry(Toolchain::Uv, "uv.lock", "u"),
            ],
        };
        prepare::store(&from, &record).unwrap_or_else(|e| panic!("{}", e.message));

        let done = seed(&from, &from, &to, &to);
        // macOS temporary folders are on APFS, which clones.
        #[cfg(target_os = "macos")]
        assert!(!done.is_empty(), "nothing was cloned on APFS");
        if done.is_empty() {
            // A filesystem that cannot clone: nothing is copied.
            assert!(!to.join("target").exists() && !to.join("node_modules").exists());
            return;
        }
        assert_eq!(done, ["target", "node_modules"]);
        assert_eq!(
            fs::read_to_string(to.join("target/debug/dep.rlib"))
                .ok()
                .as_deref(),
            Some("built")
        );
        // The lane's own files: a write there leaves the feature's alone.
        fs::write(to.join("target/debug/dep.rlib"), "rebuilt").ok();
        assert_eq!(
            fs::read_to_string(from.join("target/debug/dep.rlib"))
                .ok()
                .as_deref(),
            Some("built")
        );
        #[cfg(unix)]
        assert!(
            fs::symlink_metadata(to.join("node_modules/.bin/pad"))
                .is_ok_and(|m| m.file_type().is_symlink())
        );
        // Recorded as prepared for its lockfile; uv's folder is never cloned.
        assert!(!to.join(".venv").exists());
        let lane = prepare::load(&to);
        assert_eq!(lane.roots, vec![record.roots[0].clone()]);
    }

    #[test]
    fn nothing_is_cloned_for_another_lockfile_over_a_folder_or_through_a_link() {
        let (_d, from, to) = pair(&[("Cargo.lock", "v1")]);
        fs::write(to.join("Cargo.lock"), "v2").ok();
        fs::create_dir_all(from.join("target")).ok();
        assert!(
            seed(&from, &from, &to, &to).is_empty(),
            "another Cargo.lock"
        );

        let (_d, from, to) = pair(&[("Cargo.lock", "v1")]);
        fs::create_dir_all(from.join("target")).ok();
        fs::create_dir_all(to.join("target")).ok();
        fs::write(to.join("target/own"), "x").ok();
        assert!(
            seed(&from, &from, &to, &to).is_empty(),
            "the lane has its own"
        );
        assert!(to.join("target/own").exists());

        #[cfg(unix)]
        {
            let (d, from, to) = pair(&[("Cargo.lock", "v1")]);
            fs::create_dir_all(d.path().join("elsewhere")).ok();
            std::os::unix::fs::symlink(d.path().join("elsewhere"), from.join("target")).ok();
            assert!(
                seed(&from, &from, &to, &to).is_empty(),
                "a link is not the worktree's own"
            );
            assert!(!to.join("target").exists());
        }
    }
}
