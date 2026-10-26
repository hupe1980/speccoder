//! speccoder's window (`speccoder ui --window`): the web interface
//! in a window of its own. It loads the loopback host's page and nothing
//! else — no IPC on the data path, no other origin, no new windows. The
//! host runs in this process, so the window follows its jobs directly: when
//! a run needs the person, the Dock icon asks for attention and the title
//! says so; after every job the Dock badge counts the decisions waiting in
//! the inbox.

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
        .menu(menu)
        .on_menu_event(|app, event| {
            let id = event.id().0.as_str();
            if let Some(url) = HELP.iter().find(|(i, ..)| *i == id).map(|(_, _, u)| *u) {
                crate::open_in_browser(url);
            } else if COMMANDS.contains(&id)
                && let Some(w) = app.get_webview_window("main")
            {
                // The id is one of ours, never text from elsewhere.
                let _ = w.eval(format!(
                    "window.dispatchEvent(new CustomEvent('speccoder:menu',{{detail:'{id}'}}))"
                ));
            }
        })
        .setup(move |app| {
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(page.clone()))
                .title("speccoder")
                .inner_size(1360.0, 880.0)
                .min_inner_size(900.0, 600.0)
                // Shown once the page has drawn (its splash first): no
                // black frame while the WebView starts.
                .visible(false)
                .on_page_load(|w, payload| {
                    if payload.event() == tauri::webview::PageLoadEvent::Finished {
                        let _ = w.show();
                    }
                })
                // The host's page only: a link elsewhere is not followed.
                .on_navigation(move |u| {
                    u.scheme() == "http" && u.host_str() == Some("127.0.0.1") && u.port() == port
                })
                .build()?;
            // Should the page never say it loaded, the window shows anyway.
            if let Some(w) = app.get_webview_window("main") {
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    let _ = w.show();
                });
            }
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

/// The page's commands the menus send (the page does what the same button
/// or key does; see `command` in App.svelte).
const COMMANDS: &[&str] = &[
    "app.settings",
    "file.new",
    "file.quick",
    "file.bug",
    "file.issues",
    "file.commit",
    "view.inbox",
    "view.overview",
    "view.board",
    "view.code",
    "view.documents",
    "view.report",
    "view.sidebar",
    "view.statusbar",
    "view.actions",
    "theme.light",
    "theme.dark",
    "theme.auto",
    "run.feature",
    "run.task",
    "run.converge",
    "run.stop",
    "task.nudge",
    "task.skip",
    "task.accept",
    "task.evidence",
    "review.spec",
    "review.inbox",
    "review.pr",
    "review.report",
];

/// Help: the documentation, opened in the default browser.
const HELP: &[(&str, &str, &str)] = &[
    (
        "help.docs",
        "speccoder Documentation",
        "https://hupe1980.github.io/speccoder/docs/",
    ),
    (
        "help.quickstart",
        "Quickstart",
        "https://hupe1980.github.io/speccoder/docs/quickstart/",
    ),
    (
        "help.app",
        "The App",
        "https://hupe1980.github.io/speccoder/docs/web/",
    ),
    (
        "help.codes",
        "Error Codes",
        "https://hupe1980.github.io/speccoder/docs/codes/",
    ),
    (
        "help.issue",
        "Report an Issue…",
        "https://github.com/hupe1980/speccoder/issues/new",
    ),
];

/// The menu bar, as the HIG orders it: the app, File, Edit, View,
/// the app's own (Run, Review), Window, Help. Edit's items are the system's
/// own — they are what makes copy and paste work in the page on macOS.
fn menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem as P, Submenu};
    let item =
        |id: &str, text: &str, keys: Option<&str>| MenuItem::with_id(app, id, text, true, keys);
    let sep = || P::separator(app);

    let about = P::about(
        app,
        Some("About speccoder"),
        Some(AboutMetadata {
            name: Some("speccoder".to_owned()),
            version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            comments: Some("Spec-driven coding, decided by evidence.".to_owned()),
            website: Some("https://hupe1980.github.io/speccoder/".to_owned()),
            ..AboutMetadata::default()
        }),
    )?;
    let settings = item("app.settings", "Settings…", Some("CmdOrCtrl+,"))?;
    let app_menu = Submenu::with_items(
        app,
        "speccoder",
        true,
        &[
            &about,
            &sep()?,
            &settings,
            &sep()?,
            &P::services(app, None)?,
            &sep()?,
            &P::hide(app, None)?,
            &P::hide_others(app, None)?,
            &P::show_all(app, None)?,
            &sep()?,
            &P::quit(app, None)?,
        ],
    )?;
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &item("file.new", "New Feature…", Some("CmdOrCtrl+N"))?,
            &item("file.quick", "Quick Change…", Some("Shift+CmdOrCtrl+N"))?,
            &item("file.bug", "Report a Bug…", Some("Shift+CmdOrCtrl+B"))?,
            &item("file.issues", "Start from a GitHub Issue…", None)?,
            &sep()?,
            &item("file.commit", "Commit the Planning Files", None)?,
            &sep()?,
            &P::close_window(app, None)?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &P::undo(app, None)?,
            &P::redo(app, None)?,
            &sep()?,
            &P::cut(app, None)?,
            &P::copy(app, None)?,
            &P::paste(app, None)?,
            &P::select_all(app, None)?,
        ],
    )?;
    let appearance = Submenu::with_items(
        app,
        "Appearance",
        true,
        &[
            &item("theme.auto", "Automatic", None)?,
            &item("theme.light", "Light", None)?,
            &item("theme.dark", "Dark", None)?,
        ],
    )?;
    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &item("view.inbox", "Needs You", Some("CmdOrCtrl+1"))?,
            &item("view.overview", "Overview", Some("CmdOrCtrl+2"))?,
            &item("view.board", "Board", Some("CmdOrCtrl+3"))?,
            &item("view.code", "Code", Some("CmdOrCtrl+4"))?,
            &item("view.documents", "Spec & Plan", Some("CmdOrCtrl+5"))?,
            &item("view.report", "Report", Some("CmdOrCtrl+6"))?,
            &sep()?,
            &item("view.sidebar", "Toggle Sidebar", Some("Alt+CmdOrCtrl+S"))?,
            &item(
                "view.statusbar",
                "Toggle Status Bar",
                Some("Alt+CmdOrCtrl+B"),
            )?,
            &item("view.actions", "Actions…", Some("CmdOrCtrl+K"))?,
            &sep()?,
            &appearance,
            &sep()?,
            &P::fullscreen(app, None)?,
        ],
    )?;
    let run = Submenu::with_items(
        app,
        "Run",
        true,
        &[
            &item("run.feature", "Run the Feature", Some("CmdOrCtrl+R"))?,
            &item(
                "run.task",
                "Run the Selected Task",
                Some("Shift+CmdOrCtrl+R"),
            )?,
            &item("run.converge", "Re-check Against the Spec", None)?,
            &sep()?,
            &item(
                "task.evidence",
                "Show the Selected Task's Evidence",
                Some("CmdOrCtrl+I"),
            )?,
            &item("task.nudge", "Nudge the Selected Task…", None)?,
            &item("task.skip", "Skip the Selected Task…", None)?,
            &item("task.accept", "Accept the Selected Task…", None)?,
            &sep()?,
            &item("run.stop", "Stop", Some("CmdOrCtrl+."))?,
        ],
    )?;
    let review = Submenu::with_items(
        app,
        "Review",
        true,
        &[
            &item("review.inbox", "Needs You", Some("Shift+CmdOrCtrl+I"))?,
            &item("review.spec", "Review the Spec", Some("Alt+CmdOrCtrl+R"))?,
            &item("review.report", "Evidence Report", None)?,
            &sep()?,
            &item("review.pr", "Open Draft Pull Request…", None)?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &P::minimize(app, None)?,
            &P::maximize(app, None)?,
            &sep()?,
            &P::bring_all_to_front(app, None)?,
        ],
    )?;
    let help_items: Vec<MenuItem<tauri::Wry>> = HELP
        .iter()
        .map(|(id, text, _)| item(id, text, None))
        .collect::<tauri::Result<_>>()?;
    let help_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = help_items
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();
    let help = Submenu::with_items(app, "Help", true, &help_refs)?;
    Menu::with_items(
        app,
        &[
            &app_menu, &file, &edit, &view, &run, &review, &window, &help,
        ],
    )
}

#[cfg(test)]
mod tests {
    /// Every command a menu sends is one the page handles: a view by its
    /// id, everything else by name in App.svelte's `command`.
    #[test]
    fn every_menu_command_is_handled_by_the_page() {
        let page = include_str!("../../speccoder-web/ui/src/App.svelte");
        for id in super::COMMANDS {
            let handled = match id.strip_prefix("view.") {
                Some(view) if page.contains(&format!("v: \"{view}\"")) => true,
                _ => page.contains(&format!("case \"{id}\"")),
            };
            assert!(
                handled,
                "the menu sends `{id}` and the page does not handle it"
            );
        }
    }
}
