//! speccoder's window (`speccoder ui --window`, F-076): the web interface
//! in a window of its own. It loads the loopback host's page and nothing
//! else — no IPC on the data path, no other origin, no new windows. The
//! host runs in this process, so the window follows its jobs directly: when
//! a run needs the person, the Dock icon asks for attention and the title
//! says so; after every job the Dock badge counts the decisions waiting in
//! the inbox (F-086).

/// Opens the window on `url` (the host's page with its token) and runs
/// until it is closed. Must run on the main thread.
pub fn run(
    url: &str,
    events: Option<speccoder_web::Events>,
    root: std::path::PathBuf,
) -> Result<(), String> {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
    let page: tauri::Url = url.parse().map_err(|e| format!("{url}: {e}"))?;
    let port = page.port();
    tauri::Builder::default()
        .setup(move |app| {
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(page.clone()))
                .title("speccoder")
                .inner_size(1360.0, 880.0)
                .min_inner_size(900.0, 600.0)
                // The host's page only: a link elsewhere is not followed.
                .on_navigation(move |u| {
                    u.scheme() == "http" && u.host_str() == Some("127.0.0.1") && u.port() == port
                })
                .build()?;
            if let Some(w) = app.get_webview_window("main") {
                let root = root.clone();
                // Counting reads every feature's report: off the main thread.
                std::thread::spawn(move || badge(&w, &root));
            }
            if let Some(mut rx) = events {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    loop {
                        match rx.blocking_recv() {
                            Ok(line) => {
                                let Some(w) = handle.get_webview_window("main") else {
                                    break;
                                };
                                follow(&w, &line, &root);
                            }
                            // Fell behind: skip what was missed, keep following.
                            Err(speccoder_web::RecvError::Lagged(_)) => {}
                            Err(speccoder_web::RecvError::Closed) => break,
                        }
                    }
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .map_err(|e| e.to_string())
}

/// What one host event means for the window.
fn follow(w: &tauri::WebviewWindow, line: &str, root: &std::path::Path) {
    let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
        return;
    };
    match event["type"].as_str() {
        Some("ask" | "question") => {
            let _ = w.set_title("speccoder — needs you");
            let _ = w.request_user_attention(Some(tauri::UserAttentionType::Critical));
        }
        Some("answered") => {
            let _ = w.set_title("speccoder — running");
            let _ = w.request_user_attention(None);
        }
        Some("started") => {
            let _ = w.set_title("speccoder — running");
        }
        Some("done") => {
            let ok = event["result"]["status"] == "ok";
            let _ = w.set_title(if ok {
                "speccoder — done"
            } else {
                "speccoder"
            });
            if !w.is_focused().unwrap_or(true) {
                let _ = w.request_user_attention(Some(tauri::UserAttentionType::Informational));
            }
            badge(w, root);
        }
        _ => {}
    }
}

/// The Dock badge: how many decisions wait in the inbox; none, no badge.
fn badge(w: &tauri::WebviewWindow, root: &std::path::Path) {
    let n = match speccoder_runtime::inbox::inbox(root, None).data() {
        Some(speccoder_core::CommandData::Inbox(i)) => i.items.len(),
        _ => 0,
    };
    let _ = w.set_badge_count((n > 0).then(|| i64::try_from(n).unwrap_or(i64::MAX)));
}
