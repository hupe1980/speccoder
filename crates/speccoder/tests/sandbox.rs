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
use speccoder_runtime::verify::process::{CommandRunner, Invocation, Outcome, scrubbed_env};

/// Whether this lane is Linux without user namespaces
/// (`SPECCODER_EXPECT_FIT`): there the profile is fitted, leaving the
/// in-place carve-outs, the trust-handoff names and workspace sockets out
/// with a notice. Elsewhere only an old kernel's notice may appear.
fn fitted_lane() -> bool {
    std::env::var_os("SPECCODER_EXPECT_FIT").is_some()
}

/// The sandbox for a workspace at `root` and a home at `home`, with this
/// crate's binary as the helper; `None` (skipped) when the machine has no
/// sandbox, unless `SPECCODER_REQUIRE_SANDBOX` is set.
fn sandboxed(root: &Path, home: &Path, network: Network) -> Option<Sandboxed> {
    let config = Config {
        network,
        ..Config::default()
    };
    let profile = Profile::new(root, &config, Some(home));
    match sandbox::available() {
        Ok(()) => {}
        // CI sets SPECCODER_REQUIRE_SANDBOX: there a missing sandbox fails.
        Err(e) if std::env::var_os("SPECCODER_REQUIRE_SANDBOX").is_some() => {
            panic!("no sandbox: {e}")
        }
        Err(e) => {
            eprintln!("skipped: {e}");
            return None;
        }
    }
    let (profile, notice) =
        sandbox::fit(&profile).unwrap_or_else(|e| panic!("the sandbox cannot confine: {e}"));
    // A kernel older than 6.12 (GitHub's runners) relaxes what its Landlock
    // cannot hold on every lane; only the fitted lane gives up more.
    match (&notice, fitted_lane()) {
        (Some(n), true) => assert!(n.contains("user namespaces"), "notice: {n}"),
        (Some(n), false) => assert!(n.starts_with("this kernel is too old"), "notice: {n}"),
        (None, fitted) => assert!(!fitted, "the fitted lane fitted nothing"),
    }
    Some(Sandboxed {
        profile,
        backend: sandbox::Backend {
            helper: Some(env!("CARGO_BIN_EXE_speccoder").into()),
        },
        network_allow: Vec::new(),
        network_allow_prefix: Vec::new(),
    })
}

/// `body` with a fresh world and the sandbox, when there is one.
fn each(network: Network, body: impl Fn(&World, Sandboxed)) {
    let w = world();
    if let Some(s) = sandboxed(w.root.path(), w.home.path(), network) {
        body(&w, s);
    }
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
    each(Network::Loopback, |w, s| {
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
    });
}

/// Tools write a file in one directory and rename it into another (rustc's
/// metadata from `TMPDIR`, package managers, atomic saves): within the
/// workspace and from its temporary directory that works. `rename(2)`
/// itself, since `mv` falls back to copying.
#[test]
fn files_move_between_directories_of_the_workspace_and_its_temporary_directory() {
    each(Network::Loopback, |w, s| {
        let rename = "python3 -c 'import os,sys; os.rename(sys.argv[1], sys.argv[2])'";
        let (ok, out) = sh(
            &s,
            w.root.path(),
            &format!(
                "mkdir -p a target && echo m > a/x && {rename} a/x target/x && \
                 echo t > \"$TMPDIR/lib.rmeta\" && {rename} \"$TMPDIR/lib.rmeta\" target/lib.rmeta && \
                 cat target/x target/lib.rmeta"
            ),
        );
        assert!(ok && out.contains("m\nt"), "{out}");
    });
}

/// macOS's `mktemp` ignores `TMPDIR`: its default names in the per-user
/// temporary directory work, and nothing else there is writable.
#[cfg(target_os = "macos")]
#[test]
fn mktemp_works_and_the_rest_of_the_system_temp_stays_closed() {
    each(Network::Loopback, |w, s| {
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
    });
}

#[test]
fn credentials_are_unreadable() {
    each(Network::Loopback, |w, s| {
        let (ok, out) = sh(
            &s,
            w.root.path(),
            &format!("cat '{}/.ssh/id_ed25519'", w.home.path().display()),
        );
        assert!(!ok && !out.contains("PRIVATE"), "{out}");
    });
}

#[test]
fn the_outside_network_is_off_and_loopback_works() {
    each(Network::Loopback, |w, s| {
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
            &py(
                "import socket; c=socket.socket(socket.AF_UNIX); c.connect('/var/run/docker.sock')",
            ),
        );
        assert!(!ok, "reached the Docker socket");
    });
}

/// A daemon's Unix socket outside the workspace (Docker's is root on the
/// host) is out of reach; the same connect works unsandboxed.
#[test]
fn a_daemon_socket_outside_the_workspace_is_unreachable() {
    each(Network::Loopback, |w, s| {
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
    });
}

#[test]
fn git_push_from_a_test_fails() {
    each(Network::Loopback, |w, s| {
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
    });
}

#[cfg(target_os = "macos")]
#[test]
fn applications_cannot_be_opened() {
    each(Network::Loopback, |w, s| {
        // Launching through LaunchServices would run outside the sandbox (hidden
        // and in the background, should this test ever fail).
        let (ok, _) = sh(&s, w.root.path(), "/usr/bin/open -g -j -a Calculator");
        assert!(!ok, "opened an application outside the sandbox");
    });
}

#[test]
fn in_place_git_is_read_only_and_ignored_secrets_unreadable() {
    each(Network::Loopback, |w, mut s| {
        let root = w.root.path();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/config"), "[core]\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join(".env"), "API_KEY=secret").unwrap();
        let carved = s
            .profile
            .clone()
            .carve(&[root.join(".git")], &[root.join(".env")]);
        let (fitted, notice) = sandbox::fit(&carved).unwrap();
        s.profile = fitted;
        if fitted_lane() {
            // Without user namespaces the carve-outs are left out, by name.
            let notice = notice.unwrap();
            assert!(notice.contains("`.git`"), "{notice}");
            let (ok, out) = sh(&s, root, "echo 'pub fn x() {}' > src/x.rs");
            assert!(ok, "the code stays writable: {out}");
            let (ok, _) = sh(
                &s,
                root,
                &format!("cat {}/.ssh/id_ed25519", w.home.path().display()),
            );
            assert!(!ok, "credentials stay unreadable");
            return;
        }
        assert!(
            notice
                .as_deref()
                .is_none_or(|n| n.starts_with("this kernel is too old")),
            "{notice:?}"
        );

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
    });
}

fn run(s: &Sandboxed, root: &Path, script: &str, env: Vec<(String, String)>, secs: u64) -> Outcome {
    s.run(&Invocation {
        argv: vec!["sh".to_owned(), "-c".to_owned(), script.to_owned()],
        cwd: root.to_path_buf(),
        env,
        timeout: Duration::from_secs(secs),
        output_cap: 64 * 1024,
    })
}

fn path_only() -> Vec<(String, String)> {
    vec![("PATH".to_owned(), std::env::var("PATH").unwrap_or_default())]
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid.trim()])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The command sees the invocation's environment and the private
/// temporary directory, nothing else of the parent's.
#[test]
fn the_environment_is_the_invocations() {
    each(Network::Loopback, |w, s| {
        let mut env = path_only();
        env.push(("SPECCODER_MARK".to_owned(), "m1".to_owned()));
        let out = run(&s, w.root.path(), "env", env, 30);
        let text = String::from_utf8_lossy(&out.stdout.kept);
        assert!(text.contains("SPECCODER_MARK=m1"), "{text}");
        assert!(
            text.contains(&format!("TMPDIR={}", s.profile.tmp.display())),
            "{text}"
        );
        // Cargo sets it for this test process; the invocation omits it.
        assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some());
        assert!(!text.contains("CARGO_MANIFEST_DIR"), "{text}");
    });
}

/// What a command leaves running in its process group is killed once it
/// exits.
#[test]
fn a_background_process_dies_with_the_command() {
    each(Network::Loopback, |w, s| {
        let out = run(
            &s,
            w.root.path(),
            "sleep 30 & echo $! > bg.pid",
            path_only(),
            30,
        );
        assert_eq!(out.exit_code, Some(0), "{out:?}");
        let pid = fs::read_to_string(w.root.path().join("bg.pid")).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        assert!(!alive(&pid), "the background sleeper survived");
    });
}

/// A test runner may put each test in a process group of its own
/// (cargo-nextest does), and what it leaves there still dies with the
/// command.
#[test]
fn a_process_group_of_its_own_runs_and_dies_with_the_command() {
    each(Network::Loopback, |w, s| {
        let script = "python3 -I -c 'import os,time; os.setpgid(0, 0); open(\"pg.pid\", \"w\").write(str(os.getpid())); time.sleep(30)' & \
                      while [ ! -s pg.pid ] && kill -0 $! 2>/dev/null; do sleep 0.1; done; [ -s pg.pid ]";
        let out = run(&s, w.root.path(), script, path_only(), 30);
        assert_eq!(out.exit_code, Some(0), "{out:?}");
        let pid = fs::read_to_string(w.root.path().join("pg.pid")).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let survived = alive(&pid);
        if survived {
            let _ = std::process::Command::new("kill")
                .args(["-9", &pid])
                .status();
        }
        assert!(!survived, "a process in its own group outlived the command");
    });
}

/// At the deadline the command and its descendants are killed, also one
/// that started its own session (which would otherwise keep the output pipe
/// open and hold the outcome until it exits by itself).
#[test]
fn the_deadline_kills_the_tree() {
    each(Network::Loopback, |w, s| {
        let script = "python3 -I -c 'import os,time; os.setsid(); time.sleep(8)' & echo $! > sid.pid; sleep 60";
        let clock = std::time::Instant::now();
        let out = run(&s, w.root.path(), script, path_only(), 2);
        let took = clock.elapsed();
        assert!(out.timed_out, "{out:?}");
        let pid = fs::read_to_string(w.root.path().join("sid.pid")).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let survived = alive(&pid);
        if survived {
            let _ = std::process::Command::new("kill")
                .args(["-9", pid.trim()])
                .status();
        }
        assert!(!survived, "a new-session descendant outlived the deadline");
        assert!(took < Duration::from_secs(6), "{took:?}");
    });
}

/// An allowed command gets the network the profile denies.
#[test]
fn network_allow_opens_the_network_for_that_command() {
    each(Network::Off, |w, mut s| {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let script = format!(
            "python3 -I -c \"import socket; c=socket.socket(); c.settimeout(5); c.connect(('127.0.0.1', {port}))\""
        );
        let (ok, _) = sh(&s, w.root.path(), &script);
        assert!(!ok, "connected with the network off");
        s.network_allow = vec![vec!["sh".to_owned(), "-c".to_owned(), script.clone()]];
        let (ok, out) = sh(&s, w.root.path(), &script);
        assert!(ok, "the allowed command: {out}");
    });
}

/// The wrapped argument vector of `argv` (through the helper), or `None`
/// without a sandbox.
fn wrapped(w: &World, argv: &[&str]) -> Option<Vec<String>> {
    let s = sandboxed(w.root.path(), w.home.path(), Network::Off)?;
    fs::create_dir_all(&s.profile.tmp).unwrap();
    let argv: Vec<String> = argv.iter().map(|a| (*a).to_owned()).collect();
    Some(sandbox::wrap(&s.profile, &s.backend, &argv).unwrap())
}

fn wait_for(path: &Path) -> String {
    for _ in 0..200 {
        if let Ok(t) = fs::read_to_string(path)
            && t.ends_with('\n')
        {
            return t;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("{} never appeared", path.display());
}

/// A server started through the helper is caged by the same rules and its
/// exit status reaches the caller.
#[test]
fn the_helper_cages_a_server_and_relays_its_status() {
    let w = world();
    let outside = w.outside.path().join("escaped.txt");
    let script = format!("echo in > in.txt; echo x > '{}'; exit 3", outside.display());
    let Some(argv) = wrapped(&w, &["sh", "-c", &script]) else {
        return;
    };
    let status = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(w.root.path())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(3));
    assert!(w.root.path().join("in.txt").exists());
    assert!(!outside.exists(), "the server wrote outside the workspace");
}

/// The helper becomes the server (the same process), so SIGKILL to the
/// helper's pid ends the server; on Linux the server also dies with the
/// helper's parent.
#[test]
fn the_server_is_the_helper_and_dies_with_it() {
    let w = world();
    let server = ["sh", "-c", "echo $$ > srv.pid; exec sleep 60"];
    let Some(argv) = wrapped(&w, &server) else {
        return;
    };
    let mut helper = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(w.root.path())
        .spawn()
        .unwrap();
    let pid = wait_for(&w.root.path().join("srv.pid"));
    assert_eq!(
        pid.trim(),
        helper.id().to_string(),
        "the server is not the helper"
    );
    let kill = std::process::Command::new("kill")
        .args(["-KILL", &helper.id().to_string()])
        .status()
        .unwrap();
    assert!(kill.success());
    helper.wait().unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert!(!alive(&pid), "the server outlived SIGKILL to the helper");

    if !cfg!(target_os = "linux") {
        return;
    }
    // The helper's parent (a shell) exits after a second.
    fs::remove_file(w.root.path().join("srv.pid")).unwrap();
    let mut shell = std::process::Command::new("sh")
        .args(["-c", "\"$@\" & sleep 1", "sh"])
        .args(&argv)
        .current_dir(w.root.path())
        .spawn()
        .unwrap();
    let pid = wait_for(&w.root.path().join("srv.pid"));
    shell.wait().unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let survived = alive(&pid);
    if survived {
        let _ = std::process::Command::new("kill")
            .args(["-9", pid.trim()])
            .status();
    }
    assert!(!survived, "the server outlived the helper's parent");
}

/// A profile the helper cannot parse, or no command, runs nothing.
#[test]
fn the_helper_runs_nothing_on_a_bad_profile() {
    let w = world();
    let bin = env!("CARGO_BIN_EXE_speccoder");
    let marker = w.root.path().join("ran");
    for args in [
        vec!["{\"bogus\":1}", "--", "touch", marker.to_str().unwrap()],
        vec!["{}", "--", "touch", marker.to_str().unwrap()],
        vec![
            "{\"writable\":[],\"hidden\":[],\"read_only\":[],\"network\":\"off\",\"tmp\":\"/tmp\"}",
            "--",
        ],
    ] {
        let out = std::process::Command::new(bin)
            .arg(sandbox::HELPER_ARG)
            .args(&args)
            .output()
            .unwrap();
        assert!(!out.status.success(), "{args:?}");
        assert!(!marker.exists(), "{args:?} ran the command");
    }
}

/// A Unix socket inside the writable paths (a test server's, a database's
/// listed in `[sandbox] writable`) stays reachable.
#[test]
fn a_socket_inside_the_workspace_is_reachable() {
    each(Network::Loopback, |w, s| {
        let path = w.root.path().join("t.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let connect = format!(
            "python3 -I -c \"import socket; c=socket.socket(socket.AF_UNIX); c.connect('{}')\"",
            path.display()
        );
        let (ok, out) = sh(&s, w.root.path(), &connect);
        if fitted_lane() {
            // Without user namespaces no Unix socket is reachable (the
            // notice says so).
            assert!(!ok, "a workspace socket was reachable without namespaces");
        } else {
            assert!(ok, "the workspace socket: {out}");
        }
        drop(listener);
    });
}

/// Agent settings and git hooks in the workspace are not writable by a
/// command (they would run outside the sandbox later); ordinary files are.
/// Without user namespaces both stay writable, and the notice says so.
#[test]
fn agent_settings_and_hooks_in_the_workspace_are_not_writable() {
    each(Network::Loopback, |w, s| {
        let root = w.root.path();
        fs::create_dir_all(root.join(".claude")).unwrap();
        fs::create_dir_all(root.join("nested/.git/hooks")).unwrap();
        let (settings, _) = sh(&s, root, "echo '{}' > .claude/settings.json");
        let (hook, _) = sh(&s, root, "echo x > nested/.git/hooks/pre-commit");
        let (code, out) = sh(&s, root, "echo x > notes.txt");
        assert!(code, "an ordinary file: {out}");
        if fitted_lane() {
            assert!(settings && hook, "the notice says these stay writable here");
        } else {
            assert!(!settings && !root.join(".claude/settings.json").exists());
            assert!(!hook && !root.join("nested/.git/hooks/pre-commit").exists());
        }
    });
}

/// `s` with `.git` read-only in `root` (as speccoder carves its worktrees).
#[cfg(target_os = "macos")]
fn with_read_only_git(mut s: Sandboxed, root: &Path) -> Sandboxed {
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".git/config"), "[core]\n").unwrap();
    let carved = s.profile.clone().carve(&[root.join(".git")], &[]);
    s.profile = sandbox::fit(&carved).unwrap().0;
    s
}

/// The denial line naming `target`, if the outcome has one.
fn denial<'a>(out: &'a Outcome, target: &str) -> Option<&'a str> {
    out.sandbox
        .denied
        .iter()
        .map(String::as_str)
        .find(|l| l.contains(target))
}

/// The failed outcome of `script` whose denials name `target`, trying three
/// times: macOS drops some reports when the host is busy (this suite runs in
/// parallel), and these tests check speccoder's handling, not the kernel's.
fn failed_with(s: &Sandboxed, root: &Path, script: &str, target: &str) -> (Outcome, String) {
    let mut last = None;
    for _ in 0..3 {
        let out = run(s, root, script, path_only(), 60);
        assert_ne!(out.exit_code, Some(0), "{script} succeeded");
        if let Some(l) = denial(&out, target) {
            let l = l.to_owned();
            return (out, l);
        }
        last = Some(out.sandbox);
    }
    panic!("no denial of {target}: {last:?}")
}

/// A failed command says what the sandbox denied it and why, in
/// speccoder's words; a successful one carries nothing.
#[cfg(target_os = "macos")]
#[test]
fn a_failed_command_says_what_the_sandbox_denied() {
    each(Network::Loopback, |w, s| {
        let root = w.root.path();
        let s = with_read_only_git(s, root);
        let (_, l) = failed_with(&s, root, "echo x > .git/scratch", ".git/scratch");
        assert!(l.contains("read-only"), "{l}");
        // Git's configuration is kept from every command, read-only or not:
        // the more specific rule is the one named.
        let (_, l) = failed_with(&s, root, "echo '[evil]' >> .git/config", ".git/config");
        assert!(l.contains("git config"), "{l}");

        let key = format!("{}/.ssh/id_ed25519", w.home.path().display());
        let (_, l) = failed_with(&s, root, &format!("cat '{key}'"), ".ssh/id_ed25519");
        assert!(l.contains("hidden") || l.contains("credentials"), "{l}");

        // `nc`, not Python: Apple's Python is a shim with denials of its own.
        // Seatbelt names the port, not the address.
        let (_, l) = failed_with(
            &s,
            root,
            "/usr/bin/nc -z -G 3 1.1.1.1 443",
            "network-outbound",
        );
        assert!(l.contains("loopback only"), "{l}");

        let out = run(&s, root, "cat .git/config", path_only(), 60);
        assert_eq!(out.exit_code, Some(0));
        assert!(out.sandbox.is_empty(), "{:?}", out.sandbox);
    });
}

/// A failing check carries the denials into its result, which reports and
/// the next attempt's digest show.
#[cfg(target_os = "macos")]
#[test]
fn a_failing_check_carries_the_denials() {
    each(Network::Loopback, |w, s| {
        let root = w.root.path();
        let s = with_read_only_git(s, root);
        fs::write(
            root.join("speccoder.toml"),
            "[checks.scratch]\nargv = [\"sh\", \"-c\", \"echo x > .git/scratch\"]\n",
        )
        .unwrap();
        // Three tries: macOS drops some reports when the host is busy.
        let carried = (0..3).any(|_| {
            let report = speccoder_runtime::verify::run_checks(root, None, &s).unwrap();
            let check = &report.checks[0];
            assert!(check.status.is_failure(), "{check:?}");
            check
                .sandbox
                .denied
                .iter()
                .any(|l| l.contains(".git/scratch") && l.contains("read-only"))
        });
        assert!(carried, "the check's result never named the denial");
    });
}

/// Linux reports the system calls it refuses, and says that file and
/// network denials are not in the record.
#[cfg(target_os = "linux")]
#[test]
fn a_refused_system_call_is_named_on_linux() {
    each(Network::Loopback, |w, s| {
        let (out, l) = failed_with(&s, w.root.path(), "setsid -w true", "setsid");
        assert!(l.contains("stay in their run"), "{l}");
        let note = out.sandbox.note.clone().unwrap_or_default();
        assert!(note.contains("file and network"), "{note}");
    });
}

#[test]
fn caged_git_collects_its_garbage_in_the_foreground() {
    each(Network::Loopback, |w, s| {
        // Git's automatic `gc` would detach with `setsid`, which the cage refuses.
        let (ok, out) = sh(
            &s,
            w.root.path(),
            "git config --get gc.autoDetach 2>/dev/null",
        );
        assert!(ok && out.trim() == "false", "{out}");
    });
}

#[test]
fn a_locally_cloned_repository_runs_in_place_and_a_linked_config_does_not() {
    each(Network::Loopback, |w, mut s| {
        let root = w.root.path();
        fs::create_dir_all(root.join(".git/objects/ab")).unwrap();
        fs::write(root.join(".git/config"), "").unwrap();
        // A local `git clone` hard-links the source's objects.
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("object"), "blob").unwrap();
        fs::hard_link(
            source.path().join("object"),
            root.join(".git/objects/ab/cdef"),
        )
        .unwrap();
        let (fitted, _) =
            sandbox::fit(&s.profile.clone().carve(&[root.join(".git")], &[])).unwrap();
        s.profile = fitted;
        let (ok, out) = sh(&s, root, "echo hi");
        assert!(ok, "a cloned object store refused the run: {out}");
        if fitted_lane() {
            // `.git` is left out of the carve-outs there, so nothing is protected.
            return;
        }
        fs::hard_link(root.join(".git/config"), source.path().join("config")).unwrap();
        let (ok, out) = sh(&s, root, "echo hi");
        assert!(!ok, "a protected config with a second name ran: {out}");
        assert!(out.contains(".git/config"), "{out}");
    });
}
