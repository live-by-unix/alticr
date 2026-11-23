pub mod commands;
pub mod manager;
pub mod osc;
pub mod process_info;
pub mod session;
pub mod settings;

use manager::SessionManager;
use tauri::Manager;

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
        .setup(move |app| {
            // Wire PTY output/status/clipboard events into Tauri events.
            unsafe { manager::init_event_bridge(app.handle().clone()) };

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
