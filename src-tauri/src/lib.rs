pub mod commands;
pub mod manager;
pub mod osc;
pub mod process_info;
pub mod session;
pub mod settings;

use manager::SessionManager;
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, Runtime};

fn menu_item<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    text: &str,
    accelerator: &str,
) -> tauri::Result<tauri::menu::MenuItem<R>> {
    MenuItemBuilder::with_id(id, text)
        .accelerator(accelerator)
        .build(app)
}

/// Build the application menu. Accelerators are handled natively, so
/// shortcuts work regardless of webview focus or JS state. Menu events
/// are forwarded to the focused window as `alticr://menu-action`.
fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    let file = SubmenuBuilder::new(app, "File")
        .item(&menu_item(app, "new_tab", "New Tab", "CmdOrCtrl+T")?)
        .item(&menu_item(app, "new_window", "New Window", "CmdOrCtrl+N")?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&menu_item(app, "close_tab", "Close Tab", "CmdOrCtrl+W")?)
        .build()?;

    let edit = SubmenuBuilder::new(app, "Edit")
        .item(&menu_item(app, "copy", "Copy", "CmdOrCtrl+Shift+C")?)
        .item(&menu_item(app, "paste", "Paste", "CmdOrCtrl+Shift+V")?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&menu_item(app, "find", "Find…", "CmdOrCtrl+F")?)
        .build()?;

    let view = SubmenuBuilder::new(app, "View")
        .item(&menu_item(
            app,
            "font_larger",
            "Increase Font Size",
            "CmdOrCtrl+=",
        )?)
        .item(&menu_item(
            app,
            "font_smaller",
            "Decrease Font Size",
            "CmdOrCtrl+-",
        )?)
        .item(&menu_item(
            app,
            "font_reset",
            "Reset Font Size",
            "CmdOrCtrl+0",
        )?)
        .build()?;

    let tabs = SubmenuBuilder::new(app, "Tabs")
        .item(&menu_item(app, "next_tab", "Next Tab", "Ctrl+Tab")?)
        .item(&menu_item(
            app,
            "prev_tab",
            "Previous Tab",
            "Ctrl+Shift+Tab",
        )?)
        .build()?;

    let window = SubmenuBuilder::new(app, "Window")
        .item(&PredefinedMenuItem::minimize(app, None)?)
        .item(&PredefinedMenuItem::maximize(app, None)?)
        .build()?;

    let app_menu = SubmenuBuilder::new(app, "ALTICR")
        .item(&menu_item(
            app,
            "open_settings",
            "Settings…",
            "CmdOrCtrl+,",
        )?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&PredefinedMenuItem::about(app, None, None)?)
        .build()?;

    MenuBuilder::new(app)
        .items(&[&app_menu, &file, &edit, &view, &tabs, &window])
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let manager = std::sync::Arc::new(SessionManager::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(manager.clone())
        .on_menu_event(|app, event| {
            // Forward menu actions to the focused window only, so each
            // window's tabs/settings stay independent.
            let id = event.id().0.clone();
            for window in app.webview_windows().values() {
                if window.is_focused().unwrap_or(false) {
                    let _ = window.emit("alticr://menu-action", id.as_str());
                    break;
                }
            }
        })
        .setup(move |app| {
            // Wire PTY output/status/clipboard events into Tauri events.
            manager::init_event_bridge(app.handle().clone());
            app.set_menu(build_menu(app.handle())?)?;

            let app_handle = app.handle().clone();
            let manager = manager.clone();

            // Poll foreground process info for all sessions and emit updates.
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
                loop {
                    interval.tick().await;
                    manager.poll_foreground(&app_handle).await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::create_session,
            commands::write_session,
            commands::resize_session,
            commands::close_session,
            commands::list_sessions,
            commands::export_session,
            commands::get_settings,
            commands::set_settings,
            commands::detect_shells,
            commands::close_window_sessions,
            commands::focus_window,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                let label = window.label().to_string();
                if let Some(manager) = window
                    .app_handle()
                    .try_state::<std::sync::Arc<SessionManager>>()
                {
                    manager.close_window_sessions(&label);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running ALTICR");
}
