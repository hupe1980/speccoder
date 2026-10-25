//! speccoder's window (`speccoder ui --window`, F-076): the web interface
//! in a window of its own. It loads the loopback host's page and nothing
//! else — no IPC on the data path, no other origin, no new windows.

/// Opens the window on `url` (the host's page with its token) and runs
/// until it is closed. Must run on the main thread.
pub fn run(url: &str) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};
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
            Ok(())
        })
        .run(tauri::generate_context!())
        .map_err(|e| e.to_string())
}
