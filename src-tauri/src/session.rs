//! PTY session: one independent pseudo-terminal + shell process.
//!
//! Each `Session` owns its PTY master/slave pair, the spawned shell child
//! process, a VT100 parser (for scrollback/export), an OSC scanner (for
//! titles, cwd and clipboard sequences), and background I/O threads.

use crate::osc::OscScanner;
use crate::process_info;
use anyhow::{Context, Result};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use vt100::Parser as VtParser;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Starting,
    Running,
    Exited,
    Failed,
}

/// Serializable snapshot of a session for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub window_label: String,
    pub shell: String,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub pid: u32,
    pub created_at: u64,
    pub state: SessionState,
    pub exit_code: Option<i32>,
    pub title: String,
    pub fg_process: Option<String>,
    pub fg_pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutputEvent {
    pub session_id: String,
    pub data: String, // base64-encoded PTY output bytes
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusEvent {
    pub session_id: String,
    pub state: SessionState,
    pub exit_code: Option<i32>,
    pub title: String,
    pub cwd: String,
    pub fg_process: Option<String>,
    pub fg_pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClipboardEvent {
    pub session_id: String,
    pub text: String,
}

pub struct Session {
    pub id: String,
    pub window_label: String,
    pub shell: String,
    /// Current terminal dimensions (updated on resize).
    size: Mutex<(u16, u16)>,
    pub pid: u32,
    pub created_at: u64,

    pub state: Mutex<SessionState>,
    pub exit_code: Mutex<Option<i32>>,
    pub title: Mutex<String>,
    pub cwd: Mutex<PathBuf>,
    /// Last seen foreground process (pid, name).
    pub fg: Mutex<Option<(u32, String)>>,

    /// Wrapped in a Mutex so `Session` is `Sync` (MasterPty is only Send).
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    vt: Mutex<VtParser>,
}

pub struct SpawnOptions {
    pub shell: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub cols: u16,
    pub rows: u16,
    pub env: Vec<(String, String)>,
    pub scrollback: usize,
}

impl Session {
    /// Spawn a new PTY session running `opts.shell`.
    pub fn spawn(window_label: &str, opts: SpawnOptions) -> Result<Arc<Self>> {
        let id = format!("s{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
        let size = PtySize {
            rows: opts.rows.max(1),
            cols: opts.cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        };

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(size)
            .context("failed to open pseudo-terminal")?;

        let mut cmd = CommandBuilder::new(&opts.shell);
        cmd.args(&opts.args);
        cmd.cwd(&opts.cwd);
        for (k, v) in &opts.env {
            cmd.env(k, v);
        }
        // Ensure the child gets a sane TERM and a UTF-8 locale hint.
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        if std::env::var_os("LANG").is_none() {
            cmd.env("LANG", "en_US.UTF-8");
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| format!("failed to spawn shell '{}'", opts.shell))?;
        let pid = child.process_id().unwrap_or(0);

        let mut reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone PTY reader")?;
        let writer = pair
            .master
            .take_writer()
            .context("failed to take PTY writer")?;
        let killer = child.clone_killer();

        // The reader thread needs somewhere to send output. We store a
        // callback set by the manager after construction.
        let session = Arc::new(Self {
            id: id.clone(),
            window_label: window_label.to_string(),
            shell: opts.shell.clone(),
            size: Mutex::new((opts.cols, opts.rows)),
            pid,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            state: Mutex::new(SessionState::Running),
            exit_code: Mutex::new(None),
            title: Mutex::new(String::new()),
            cwd: Mutex::new(opts.cwd.clone()),
            fg: Mutex::new(None),
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            killer: Mutex::new(killer),
            vt: Mutex::new(VtParser::new(opts.rows, opts.cols, opts.scrollback)),
        });

        // Reader thread: PTY output -> OSC scan -> vt100 -> emit event.
        let s = Arc::clone(&session);
        std::thread::Builder::new()
            .name(format!("pty-reader-{id}"))
            .spawn(move || reader_loop(s, &mut *reader))
            .context("failed to spawn PTY reader thread")?;

        // Waiter thread: reaps the child and records exit status.
        let s = Arc::clone(&session);
        std::thread::Builder::new()
            .name(format!("pty-waiter-{id}"))
            .spawn(move || {
                let mut child = child;
                let status = child.wait();
                let code = status.as_ref().ok().map(|st| st.exit_code() as i32);
                *s.exit_code.lock().unwrap() = code;
                *s.state.lock().unwrap() = match status {
                    Ok(_) => SessionState::Exited,
                    Err(_) => SessionState::Failed,
                };
                if let Some(cb) = EXIT_CALLBACK.lock().unwrap().as_ref() {
                    cb(s.clone());
                }
            })
            .context("failed to spawn PTY waiter thread")?;

        Ok(session)
    }

    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut w = self.writer.lock().unwrap();
        w.write_all(data).context("failed to write to PTY")?;
        w.flush().ok();
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let size = PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        };
        self.master
            .lock()
            .unwrap()
            .resize(size)
            .context("failed to resize PTY")?;
        self.vt.lock().unwrap().set_size(rows, cols);
        *self.size.lock().unwrap() = (cols, rows);
        Ok(())
    }

    pub fn kill(&self) {
        let mut k = self.killer.lock().unwrap();
        let _ = k.kill();
    }

    /// Raw fd of the PTY master, for foreground-process detection.
    #[cfg(unix)]
    pub fn master_fd(&self) -> Option<std::os::unix::io::RawFd> {
        self.master.lock().unwrap().as_raw_fd()
    }

    #[cfg(not(unix))]
    pub fn master_fd(&self) -> Option<i32> {
        None
    }

    pub fn is_running(&self) -> bool {
        *self.state.lock().unwrap() == SessionState::Running
            || *self.state.lock().unwrap() == SessionState::Starting
    }

    pub fn snapshot(&self) -> SessionInfo {
        let fg = self.fg.lock().unwrap().clone();
        let (cols, rows) = *self.size.lock().unwrap();
        SessionInfo {
            id: self.id.clone(),
            window_label: self.window_label.clone(),
            shell: self.shell.clone(),
            cwd: self.cwd.lock().unwrap().to_string_lossy().into_owned(),
            cols,
            rows,
            pid: self.pid,
            created_at: self.created_at,
            state: *self.state.lock().unwrap(),
            exit_code: *self.exit_code.lock().unwrap(),
            title: self.title.lock().unwrap().clone(),
            fg_process: fg.as_ref().map(|(_, n)| n.clone()),
            fg_pid: fg.map(|(p, _)| p),
        }
    }

    /// Plain-text contents of the terminal screen + scrollback.
    pub fn export_text(&self) -> String {
        self.vt.lock().unwrap().screen().contents()
    }
}

fn reader_loop(session: Arc<Session>, reader: &mut dyn Read) {
    let mut buf = [0u8; 8192];
    let mut osc = OscScanner::new();

    loop {
        match reader.read(&mut buf) {
            Ok(0) => break, // EOF: slave side closed
            Ok(n) => {
                let chunk = &buf[..n];

                // Feed the VT parser for scrollback/export.
                {
                    let mut vt = session.vt.lock().unwrap();
                    vt.process(chunk);
                }

                // Scan for OSC sequences (titles, cwd, clipboard).
                for seq in osc.feed(chunk) {
                    handle_osc(&session, &seq);
                }

                // Emit raw bytes to the frontend for rendering.
                if let Some(cb) = OUTPUT_CALLBACK.lock().unwrap().clone() {
                    cb(&session.id, chunk.to_vec());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
}

fn handle_osc(session: &Arc<Session>, seq: &crate::osc::OscSequence) {
    match seq.ps {
        0 | 1 | 2 => {
            // Window / icon / tab title.
            if !seq.pt.is_empty() {
                *session.title.lock().unwrap() = seq.pt.clone();
            }
        }
        7 => {
            // cwd reporting: file://host/path
            if let Some(p) = process_info::parse_osc7_cwd(&seq.pt) {
                *session.cwd.lock().unwrap() = p;
            }
        }
        52 => {
            // Clipboard. Payload: c;<base64-or-text>. Forward to the
            // frontend which enforces the user's clipboard policy.
            if let Some(cb) = CLIPBOARD_CALLBACK.lock().unwrap().clone() {
                cb(session.id.clone(), seq.pt.clone());
            }
        }
        _ => {}
    }
}

// ---- Callback plumbing -------------------------------------------------
//
// The reader/waiter threads can't hold a Tauri AppHandle directly without
// complicating lifetimes, so the manager installs lightweight callbacks
// that route events back into Tauri's event system.

pub type OutputCb = Arc<dyn Fn(&str, Vec<u8>) + Send + Sync>;
pub type StatusCb = Arc<dyn Fn(Arc<Session>) + Send + Sync>;
pub type ClipboardCb = Arc<dyn Fn(String, String) + Send + Sync>;

static OUTPUT_CALLBACK: Mutex<Option<OutputCb>> = Mutex::new(None);
static EXIT_CALLBACK: Mutex<Option<StatusCb>> = Mutex::new(None);
static CLIPBOARD_CALLBACK: Mutex<Option<ClipboardCb>> = Mutex::new(None);

pub fn set_callbacks(output: OutputCb, exit: StatusCb, clipboard: ClipboardCb) {
    *OUTPUT_CALLBACK.lock().unwrap() = Some(output);
    *EXIT_CALLBACK.lock().unwrap() = Some(exit);
    *CLIPBOARD_CALLBACK.lock().unwrap() = Some(clipboard);
}
