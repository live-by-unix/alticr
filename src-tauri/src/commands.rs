//! Tauri IPC commands exposed to the frontend.

use crate::manager::SessionManager;
use crate::settings::AppSettings;
use serde::Deserialize;
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Deserialize)]
pub struct CreateSessionArgs {
    pub window_label: String,
    pub shell: Option<String>,
    pub args: Option<String>,
    pub cwd: Option<String>,
    pub cols: u16,
    pub rows: u16,
    #[serde(default)]
    pub env: Vec<(String, String)>,
}

#[tauri::command]
pub fn create_session(
    app: AppHandle,
    manager: State<'_, std::sync::Arc<SessionManager>>,
    args: CreateSessionArgs,
) -> Result<crate::session::SessionInfo, String> {
    // Ensure the requesting window exists so sessions can be cleaned up.
    if app.get_webview_window(&args.window_label).is_none() {
        return Err(format!("unknown window '{}'", args.window_label));
    }
    manager
        .create_session(
            &args.window_label,
            args.shell,
            args.args,
            args.cwd,
            args.cols.max(1),
            args.rows.max(1),
            args.env,
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_session(
    manager: State<'_, std::sync::Arc<SessionManager>>,
    session_id: String,
    data_b64: String,
) -> Result<(), String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_b64)
        .map_err(|e| format!("invalid base64 input: {e}"))?;
    let session = manager
        .get(&session_id)
        .ok_or_else(|| format!("unknown session '{session_id}'"))?;
    session.write(&bytes).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resize_session(
    manager: State<'_, std::sync::Arc<SessionManager>>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let session = manager
        .get(&session_id)
        .ok_or_else(|| format!("unknown session '{session_id}'"))?;
    session.resize(cols, rows).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn close_session(
    manager: State<'_, std::sync::Arc<SessionManager>>,
    session_id: String,
) -> Result<(), String> {
    manager
        .close_session(&session_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_sessions(
    manager: State<'_, std::sync::Arc<SessionManager>>,
    window_label: String,
) -> Vec<crate::session::SessionInfo> {
    manager.list_for_window(&window_label)
}

#[tauri::command]
pub fn export_session(
    manager: State<'_, std::sync::Arc<SessionManager>>,
    session_id: String,
) -> Result<String, String> {
    let session = manager
        .get(&session_id)
        .ok_or_else(|| format!("unknown session '{session_id}'"))?;
    Ok(session.export_text())
}

#[tauri::command]
pub fn get_settings(manager: State<'_, std::sync::Arc<SessionManager>>) -> AppSettings {
    manager.settings.get()
}

#[tauri::command]
pub fn set_settings(
    manager: State<'_, std::sync::Arc<SessionManager>>,
    settings: serde_json::Value,
) -> Result<AppSettings, String> {
    manager.settings.update(settings).map_err(|e| e.to_string())
}

/// List shells commonly available on this platform.
#[tauri::command]
pub fn detect_shells() -> Vec<ShellInfo> {
    let mut out = Vec::new();
    #[cfg(unix)]
    {
        for path in [
            "/bin/zsh",
            "/bin/bash",
            "/bin/sh",
            "/bin/fish",
            "/usr/local/bin/fish",
            "/opt/homebrew/bin/fish",
        ] {
            if std::path::Path::new(path).exists() {
                let name = std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string());
                out.push(ShellInfo {
                    name,
                    path: path.to_string(),
                    is_default: false,
                });
            }
        }
        // Also offer $SHELL if it isn't already listed.
        if let Ok(s) = std::env::var("SHELL") {
            if !out.iter().any(|i| i.path == s) {
                let name = std::path::Path::new(&s)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| s.clone());
                out.insert(
                    0,
                    ShellInfo {
                        name,
                        path: s,
                        is_default: true,
                    },
                );
            }
        }
    }
    #[cfg(windows)]
    {
        for (name, path) in [
            ("PowerShell", "powershell.exe"),
            ("PowerShell 7", "pwsh.exe"),
            ("Command Prompt", "cmd.exe"),
        ] {
            out.push(ShellInfo {
                name: name.to_string(),
                path: path.to_string(),
                is_default: path == "powershell.exe",
            });
        }
    }
    out
}

#[derive(Debug, serde::Serialize)]
pub struct ShellInfo {
    pub name: String,
    pub path: String,
    pub is_default: bool,
}

/// Kill all sessions owned by a window (called when the window closes).
#[tauri::command]
pub fn close_window_sessions(
    manager: State<'_, std::sync::Arc<SessionManager>>,
    window_label: String,
) -> Result<(), String> {
    manager.close_window_sessions(&window_label);
    Ok(())
}

/// Bring an existing window to the front (used by "New Window" flows).
#[tauri::command]
pub fn focus_window(app: AppHandle, label: String) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(&label) {
        w.set_focus().map_err(|e| e.to_string())?;
        w.show().map_err(|e| e.to_string())?;
    }
    Ok(())
}
