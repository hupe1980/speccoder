//! Ctrl-C during a run, in two steps: the first stops the running attempt
//! before its next model turn or tool call (the call in flight completes,
//! the attempt is saved as stopped) and no further task starts; the second
//! quits at once, as Ctrl-C always did.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// What a press of Ctrl-C does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// The first: stop at the next call.
    Stop,
    /// Any later one: quit now.
    Quit,
}

/// The `n`th press (1-based).
#[must_use]
pub const fn press(n: u32) -> Press {
    if n <= 1 { Press::Stop } else { Press::Quit }
}

/// Listens for Ctrl-C while a run of the project at `root` goes on; sets
/// `cancel` on the first press.
pub fn two_step(root: &Path, cancel: Arc<AtomicBool>) {
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        rt.block_on(async {
            let mut n = 0;
            while tokio::signal::ctrl_c().await.is_ok() {
                n += 1;
                match press(n) {
                    Press::Stop => {
                        cancel.store(true, Ordering::SeqCst);
                        // Off this loop: a prompt holds stderr while it reads,
                        // and the next press must still be heard.
                        let root = root.clone();
                        std::thread::spawn(move || {
                            let _ = speccoder_runtime::service::thread::stop(&root, "terminal");
                            eprintln!(
                                "\nstopping after the call in flight — Ctrl-C again to quit now"
                            );
                        });
                    }
                    // At once, before any output that could wait on a lock.
                    Press::Quit => std::process::exit(130),
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_press_stops_and_the_next_quits() {
        assert_eq!(press(1), Press::Stop);
        assert_eq!(press(2), Press::Quit);
        assert_eq!(press(5), Press::Quit);
    }
}
