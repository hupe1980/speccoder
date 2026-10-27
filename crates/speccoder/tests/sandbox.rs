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
        network_allow_prefix: Vec::new(),
        env: Vec::new(),
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

/// macOS's `mktemp` ignores `TMPDIR`: its default names in the per-user
/// temporary directory work, and nothing else there is writable.
#[cfg(target_os = "macos")]
#[test]
fn mktemp_works_and_the_rest_of_the_system_temp_stays_closed() {
    let w = world();
    let Some(s) = sandboxed(w.root.path(), w.home.path(), Network::Loopback) else {
        return;
    };
    let (ok, out) = sh(
        &s,
        w.root.path(),
        "f=$(mktemp) && echo x > \"$f\" && d=$(mktemp -d) && echo y > \"$d/inner\" && rm -rf \"$f\" \"$d\" && echo ok",
    );
    assert!(ok && out.contains("ok"), "{out}");
    let other = std::env::temp_dir().join(format!("speccoder-escape-{}", std::process::id()));
    let (ok, _) = sh(
        &s,
        w.root.path(),
        &format!("echo x > '{}'", other.display()),
    );
    assert!(
        !ok && !other.exists(),
        "wrote another name in the system temp"
    );
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

/// A daemon's Unix socket outside the workspace (Docker's is root on the
/// host) is out of reach; the same connect works unsandboxed.
#[test]
fn a_daemon_socket_outside_the_workspace_is_unreachable() {
    let w = world();
    let Some(s) = sandboxed(w.root.path(), w.home.path(), Network::Loopback) else {
        return;
    };
    let path = w.outside.path().join("d.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let connect = format!(
        "python3 -I -c \"import socket; c=socket.socket(socket.AF_UNIX); c.connect('{}')\"",
        path.display()
    );
    let free = std::process::Command::new("sh")
        .args(["-c", &connect])
        .status()
        .unwrap();
    assert!(free.success(), "the socket works unsandboxed");
    let (ok, out) = sh(&s, w.root.path(), &connect);
    assert!(!ok, "reached a daemon socket: {out}");
    drop(listener);
}

#[test]
fn git_push_from_a_test_fails() {
    let w = world();
    let Some(s) = sandboxed(w.root.path(), w.home.path(), Network::Loopback) else {
        return;
    };
    let remote = w.outside.path().join("remote.git");
    let ok = std::process::Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
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

#[test]
fn in_place_git_is_read_only_and_ignored_secrets_unreadable() {
    let w = world();
    let Some(mut s) = sandboxed(w.root.path(), w.home.path(), Network::Loopback) else {
        return;
    };
    let root = w.root.path();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".git/config"), "[core]\n").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join(".env"), "API_KEY=secret").unwrap();
    s.profile = s.profile.carve(&[root.join(".git")], &[root.join(".env")]);

    let (ok, out) = sh(&s, root, "echo '[evil]' >> .git/config");
    assert!(!ok, "wrote .git/config: {out}");
    let (ok, _) = sh(&s, root, "echo x > .git/hooks-pre-commit");
    assert!(
        !ok && !root.join(".git/hooks-pre-commit").exists(),
        "created a file in .git"
    );
    assert_eq!(
        fs::read_to_string(root.join(".git/config")).unwrap(),
        "[core]\n"
    );
    let (ok, out) = sh(&s, root, "cat .git/config");
    assert!(
        ok && out.contains("[core]"),
        "git still reads its repository: {out}"
    );
    // Refused (macOS) or read as empty (Linux masks it with /dev/null).
    let (_, out) = sh(&s, root, "cat .env");
    assert!(!out.contains("secret"), "read the ignored secret: {out}");
    let (ok, out) = sh(&s, root, "echo 'pub fn x() {}' > src/x.rs && cat src/x.rs");
    assert!(ok, "the code stays writable: {out}");
}
