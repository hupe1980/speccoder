//! Ending a started command and everything it started. Through the sandbox
//! helper the command leads a session of its own (procage's
//! `exec_session`): what it starts may form process groups (test runners)
//! but never leave the session. A plain command leads a process group.

/// Ends every process of the session `pid` leads, then its process group.
/// Called before reaping `pid` or soon after: while any member lives the id
/// is no other session's, and once none does there is nothing to end (pids
/// are handed out in sequence, so another session with it is far off).
pub fn end(pid: u32) {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    procage::native::end_session(pid);
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    if let Ok(pid) = i32::try_from(pid) {
        let _ = nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(pid),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
}
