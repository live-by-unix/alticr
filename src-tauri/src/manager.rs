//! SessionManager: owns all live PTY sessions across all windows.

use crate::process_info;
use crate::session::{self, Session, SessionInfo, SpawnOptions};
use crate::settings::SettingsStore;
use anyhow::{Context, Result};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

pub struct SessionManager {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    /// window label -> session ids, for cleanup when a window closes.
    windows: Mutex<HashMap<String, Vec<String>>>,
    pub settings: SettingsStore,
}

impl SessionManager {
    pub fn new() -> Self {
        let settings = SettingsStore::load().unwrap_or_else(|e| {
            tracing::error!("failed to load settings: {e}; using in-memory defaults");
            SettingsStore::in_memory()
        });

        Self {
            sessions: Mutex::new(HashMap::new()),
            windows: Mutex::new(HashMap::new()),
            settings,
        }
    }

    fn register(&self, session: Arc<Session>) {
        let id = session.id.clone();
        let win = session.window_label.clone();
        self.sessions.lock().insert(id.clone(), session);
        self.windows.lock().entry(win).or_default().push(id);
    }

    fn unregister(&self, id: &str) {
        self.sessions.lock().remove(id);
        self.windows
            .lock()
            .values_mut()
            .for_each(|v| v.retain(|x| x != id));
    }

    pub fn get(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions.lock().get(id).cloned()
    }

    pub fn list_for_window(&self, window_label: &str) -> Vec<SessionInfo> {
        let mut v: Vec<SessionInfo> = self
            .sessions
            .lock()
            .values()
            .filter(|s| s.window_label == window_label)
            .map(|s| s.snapshot())
            .collect();
        v.sort_by_key(|s| s.created_at);
        v
    }

    pub fn all(&self) -> Vec<SessionInfo> {
        self.sessions
            .lock()
            .values()
            .map(|s| s.snapshot())
            .collect()
    }

    pub fn close_session(&self, id: &str) -> Result<()> {
        if let Some(s) = self.get(id) {
            s.kill();
        }
        self.unregister(id);
        Ok(())
    }

    /// Kill every session belonging to a window (called on window close).
    pub fn close_window_sessions(&self, window_label: &str) {
        let ids: Vec<String> = self
            .windows
            .lock()
            .get(window_label)
            .cloned()
            .unwrap_or_default();
        for id in ids {
            if let Some(s) = self.sessions.lock().remove(&id) {
                s.kill();
            }
        }
        self.windows.lock().remove(window_label);
    }

    /// Create a session. Returns the new session info.
    pub fn create_session(
        &self,
        window_label: &str,
        shell: Option<String>,
        args: Option<String>,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
        env: Vec<(String, String)>,
    ) -> Result<SessionInfo> {
        let settings = self.settings.get();

        let shell_exe = shell
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| detect_default_shell(&settings.default_shell));

        let arg_vec: Vec<String> = match args {
            Some(a) if !a.trim().is_empty() => shell_words::split(&a).unwrap_or_default(),
            _ => shell_words::split(&settings.shell_args).unwrap_or_default(),
        };

        let cwd_path = cwd
            .filter(|c| !c.trim().is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                let d = settings.startup_dir.trim();
                if d.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(d))
                }
            })
            .unwrap_or_else(home_dir);

        let mut env_pairs: Vec<(String, String)> = settings
            .env_vars
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        env_pairs.extend(env);

        let session = Session::spawn(
            window_label,
            SpawnOptions {
                shell: shell_exe.clone(),
                args: arg_vec,
                cwd: cwd_path,
                cols,
                rows,
                env: env_pairs,
                scrollback: settings.scrollback_lines,
            },
        )
        .with_context(|| format!("failed to create session for '{shell_exe}'"))?;

        let info = session.snapshot();
        self.register(session);
        Ok(info)
    }

    /// Poll all sessions for foreground-process / cwd changes and emit
    /// status events when something changed.
    pub async fn poll_foreground(&self, app: &AppHandle) {
        #[cfg(unix)]
        let pgroups = process_info::process_groups();
        #[cfg(not(unix))]
        let pgroups: HashMap<i32, Vec<(u32, String)>> = HashMap::new();

        let sessions: Vec<Arc<Session>> = self.sessions.lock().values().cloned().collect();
        for s in sessions {
            if !s.is_running() {
                continue;
            }
            #[cfg(unix)]
            {
                if let Some(fd) = s.master_fd() {
                    let pgid = unsafe { libc::tcgetpgrp(fd) };
                    if pgid > 0 {
                        if let Some((pid, name)) = pgroups.get(&pgid).and_then(|v| {
                            // Prefer the process-group leader.
                            v.iter()
                                .find(|(p, _)| *p as i32 == pgid)
                                .or_else(|| v.first())
                                .cloned()
                        }) {
                            let mut fg = s.fg.lock().unwrap();
                            let same = fg
                                .as_ref()
                                .map(|(p, n)| *p == pid && *n == name)
                                .unwrap_or(false);
                            if !same {
                                *fg = Some((pid, name));
                                drop(fg);
                                if let Some(cwd) = process_info::process_cwd(pid) {
                                    *s.cwd.lock().unwrap() = cwd;
                                }
                                emit_status(app, &s);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn emit_status(app: &AppHandle, s: &Session) {
    let snap = s.snapshot();
    let _ = app.emit(
        "alticr://status",
        session::StatusEvent {
            session_id: snap.id.clone(),
            state: snap.state,
            exit_code: snap.exit_code,
            title: snap.title,
            cwd: snap.cwd,
            fg_process: snap.fg_process,
            fg_pid: snap.fg_pid,
        },
    );
}

fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

/// Determine the default shell: explicit config > $SHELL > login shell.
fn detect_default_shell(configured: &str) -> String {
    if !configured.trim().is_empty() {
        return configured.trim().to_string();
    }
    #[cfg(unix)]
    {
        if let Ok(s) = std::env::var("SHELL") {
            if !s.is_empty() {
                return s;
            }
        }
        if let Some(pw) = user_shell() {
            return pw;
        }
        "/bin/sh".to_string()
    }
    #[cfg(windows)]
    {
        for candidate in ["pwsh.exe", "powershell.exe"] {
            if which_exists(candidate) {
                return candidate.to_string();
            }
        }
        "cmd.exe".to_string()
    }
}

#[cfg(unix)]
fn user_shell() -> Option<String> {
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() {
            return None;
        }
        let shell = std::ffi::CStr::from_ptr((*pw).pw_shell);
        let s = shell.to_string_lossy().into_owned();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
}

#[cfg(windows)]
fn which_exists(name: &str) -> bool {
    if let Ok(paths) = std::env::var("PATH") {
        for dir in std::env::split_paths(&paths) {
            if dir.join(name).exists() {
                return true;
            }
        }
    }
    false
}

// ---- Event bridge --------------------------------------------------------
//
// Session reader/waiter threads were spawned before the Tauri AppHandle
// existed, so output/status/clipboard routing is installed once during
// setup() via `init_event_bridge`.

static APP_PTR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Wire session callbacks to Tauri's event emitter. Called once from
/// setup() after the AppHandle exists.
///
/// # Safety
/// The callbacks capture a raw pointer to the AppHandle, which is valid
/// for the lifetime of the application.
pub unsafe fn init_event_bridge(app: AppHandle) {
    APP_PTR.store(
        Box::into_raw(Box::new(app)) as usize,
        std::sync::atomic::Ordering::SeqCst,
    );

    fn app() -> &'static AppHandle {
        unsafe { &*(APP_PTR.load(std::sync::atomic::Ordering::SeqCst) as *const AppHandle) }
    }

    let output_cb: session::OutputCb = Arc::new(|id, data| {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
        let _ = app().emit(
            "alticr://output",
            session::OutputEvent {
                session_id: id.to_string(),
                data: b64,
            },
        );
    });

    let status_cb: session::StatusCb = Arc::new(|s| {
        emit_status(app(), &s);
    });

    let clipboard_cb: session::ClipboardCb = Arc::new(|id, text| {
        let _ = app().emit(
            "alticr://clipboard",
            session::ClipboardEvent {
                session_id: id,
                text,
            },
        );
    });

    session::set_callbacks(output_cb, status_cb, clipboard_cb);
}
