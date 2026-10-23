//! The sandbox escape suite:
//! writes outside the workspace, reading credentials, the network, the
//! Docker socket, `git push` from a test and opening applications all fail
//! closed; work inside the workspace and loopback servers still work.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::time::Duration;

use speccoder_runtime::sandbox::{self, Config, Network, Profile, Sandboxed};
use speccoder_runtime::verify::process::{CommandRunner, Invocation, scrubbed_env};

fn sandboxed(root: &Path, home: &Path, network: Network) -> Option<Sandboxed> {
    // On Linux the helper is this crate's binary.
    let backend = if cfg!(target_os = "linux") {
        sandbox::with_helper(env!("CARGO_BIN_EXE_speccoder").into())
    } else {
        sandbox::backend()
    };
    let backend = match backend {
        Ok(b) => b,
        // CI sets SPECCODER_REQUIRE_SANDBOX: there a missing sandbox fails.
        Err(e) if std::env::var_os("SPECCODER_REQUIRE_SANDBOX").is_some() => {
            panic!("no sandbox: {e}")
        }
        Err(e) => {
            eprintln!("skipped: {e}");
            return None;
        }
    };
    let config = Config {
        network,
        ..Config::default()
    };
    Some(Sandboxed {
        profile: Profile::new(root, &config, Some(home)),
        backend,
        network_allow: Vec::new(),
    })
}

fn sh(s: &Sandboxed, root: &Path, script: &str) -> (bool, String) {
    let out = s.run(&Invocation {
        argv: vec!["sh".to_owned(), "-c".to_owned(), script.to_owned()],
        cwd: root.to_path_buf(),
        env: scrubbed_env(std::env::vars()),
        timeout: Duration::from_secs(60),
        output_cap: 64 * 1024,
    });
    let text = format!(
        "{}{}{}",
        out.spawn_error.unwrap_or_default(),
        String::from_utf8_lossy(&out.stdout.kept),
        String::from_utf8_lossy(&out.stderr.kept)
    );
    (out.exit_code == Some(0), text)
}

struct World {
    root: tempfile::TempDir,
    home: tempfile::TempDir,
    outside: tempfile::TempDir,
}

fn world() -> World {
    let w = World {
        root: tempfile::tempdir().unwrap(),
        home: tempfile::tempdir().unwrap(),
        outside: tempfile::tempdir().unwrap(),
    };
    fs::create_dir_all(w.home.path().join(".ssh")).unwrap();
    fs::write(w.home.path().join(".ssh/id_ed25519"), "PRIVATE").unwrap();
    w
}

#[test]
fn writes_stay_in_the_workspace_and_its_temporary_directory() {
    let w = world();
    let Some(s) = sandboxed(w.root.path(), w.home.path(), Network::Loopback) else {
        return;
    };
    let (ok, out) = sh(
        &s,
        w.root.path(),
        "echo hi > inside.txt && echo t > \"$TMPDIR/t\" && cat inside.txt",
    );
    assert!(ok, "{out}");
    let outside = w.outside.path().join("escaped.txt");
    let (ok, _) = sh(
        &s,
        w.root.path(),
        &format!("echo x > '{}'", outside.display()),
    );
    assert!(!ok && !outside.exists(), "wrote outside the workspace");
    let (ok, _) = sh(
        &s,
        w.root.path(),
        &format!("echo x > '{}/.bashrc'", w.home.path().display()),
    );
    assert!(!ok, "wrote into the home directory");
}

#[test]
fn credentials_are_unreadable() {
    let w = world();
    let Some(s) = sandboxed(w.root.path(), w.home.path(), Network::Loopback) else {
        return;
    };
    let (ok, out) = sh(
        &s,
        w.root.path(),
        &format!("cat '{}/.ssh/id_ed25519'", w.home.path().display()),
    );
    assert!(!ok && !out.contains("PRIVATE"), "{out}");
}

#[test]
fn the_outside_network_is_off_and_loopback_works() {
    let w = world();
    let Some(s) = sandboxed(w.root.path(), w.home.path(), Network::Loopback) else {
        return;
    };
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let py = |code: &str| format!("python3 -I -c \"{code}\"");
    let (ok, out) = sh(
        &s,
        w.root.path(),
        &py("import socket; c=socket.socket(); c.settimeout(5); c.connect(('1.1.1.1', 443))"),
    );
    assert!(!ok, "connected to the outside: {out}");
    let (ok, out) = sh(
        &s,
        w.root.path(),
        &py(&format!(
            "import socket; c=socket.socket(); c.settimeout(5); c.connect(('127.0.0.1', {port}))"
        )),
    );
    if cfg!(target_os = "macos") {
        assert!(ok, "loopback: {out}");
    } else {
        // Linux: seccomp cannot tell loopback from the outside.
        assert!(!ok, "{out}");
    }
    drop(listener);
    let (ok, _) = sh(
        &s,
        w.root.path(),
        &py("import socket; c=socket.socket(socket.AF_UNIX); c.connect('/var/run/docker.sock')"),
    );
    assert!(!ok, "reached the Docker socket");
}

#[test]
fn git_push_from_a_test_fails() {
    let w = world();
    let Some(s) = sandboxed(w.root.path(), w.home.path(), Network::Loopback) else {
        return;
    };
    let remote = w.outside.path().join("remote.git");
    let ok = std::process::Command::new("git")
        .args(["init", "-q", "--bare"])
        .arg(&remote)
        .status()
        .unwrap()
        .success();
    assert!(ok);
    let script = format!(
        "git init -q . && git -c user.name=t -c user.email=t@e commit -q --allow-empty -m x && git push -q '{}' HEAD:refs/heads/main",
        remote.display()
    );
    let (ok, out) = sh(&s, w.root.path(), &script);
    assert!(!ok, "pushed: {out}");
    let refs = fs::read_dir(remote.join("refs/heads")).unwrap().count();
    assert_eq!(refs, 0, "{out}");
}

#[cfg(target_os = "macos")]
#[test]
fn applications_cannot_be_opened() {
    let w = world();
    let Some(s) = sandboxed(w.root.path(), w.home.path(), Network::Loopback) else {
        return;
    };
    // Launching through LaunchServices would run outside the sandbox (hidden
    // and in the background, should this test ever fail).
    let (ok, _) = sh(&s, w.root.path(), "/usr/bin/open -g -j -a Calculator");
    assert!(!ok, "opened an application outside the sandbox");
}
