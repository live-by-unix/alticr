//! Persisted application settings.
//!
//! Settings live in a single JSON file inside the platform config
//! directory (`~/Library/Application Support/ALTICR/settings.json` on
//! macOS, `$XDG_CONFIG_HOME/ALTICR/settings.json` on Linux, etc.).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    // Appearance
    pub theme: String,
    pub accent_color: String,
    pub font_family: String,
    pub font_size: f64,
    pub line_height: f64,
    pub cursor_style: String,
    pub cursor_blink: bool,
    pub window_opacity: f64,
    pub padding: u32,

    // Terminal
    pub default_shell: String,
    pub shell_args: String,
    pub startup_dir: String,
    pub scrollback_lines: usize,
    pub env_vars: BTreeMap<String, String>,
    pub bell: String,

    // Tabs
    pub tab_show_process: bool,
    pub tab_show_shell: bool,
    pub confirm_close_running: bool,

    // Keyboard — map of action name to accelerator string.
    pub shortcuts: BTreeMap<String, String>,

    // Advanced
    pub allow_osc52_clipboard: bool,
    pub debug_logging: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        let mut shortcuts = BTreeMap::new();
        shortcuts.insert("new_tab".into(), "mod+t".into());
        shortcuts.insert("close_tab".into(), "mod+w".into());
        shortcuts.insert("new_window".into(), "mod+n".into());
        shortcuts.insert("next_tab".into(), "mod+tab".into());
        shortcuts.insert("prev_tab".into(), "mod+shift+tab".into());
        shortcuts.insert("copy".into(), "mod+shift+c".into());
        shortcuts.insert("paste".into(), "mod+shift+v".into());
        shortcuts.insert("find".into(), "mod+f".into());
        shortcuts.insert("font_larger".into(), "mod+=".into());
        shortcuts.insert("font_smaller".into(), "mod+-".into());
        shortcuts.insert("font_reset".into(), "mod+0".into());
        shortcuts.insert("select_all".into(), "mod+a".into());

        Self {
            theme: "dark".into(),
            accent_color: "#38bdf8".into(),
            font_family: "".into(), // empty = system monospace default
            font_size: 13.0,
            line_height: 1.0,
            cursor_style: "block".into(),
            cursor_blink: true,
            window_opacity: 1.0,
            padding: 8,
            default_shell: String::new(), // empty = auto-detect
            shell_args: String::new(),
            startup_dir: String::new(), // empty = home dir
            scrollback_lines: 10000,
            env_vars: BTreeMap::new(),
            bell: "visual".into(),
            tab_show_process: true,
            tab_show_shell: true,
            confirm_close_running: true,
            shortcuts,
            allow_osc52_clipboard: false,
            debug_logging: false,
        }
    }
}

/// Thread-safe settings store backed by a JSON file.
pub struct SettingsStore {
    #[allow(dead_code)]
    path: PathBuf,
    inner: Mutex<AppSettings>,
}

impl SettingsStore {
    /// In-memory store used as a fallback when the config dir is
    /// unavailable (e.g. headless tests).
    pub fn in_memory() -> Self {
        Self {
            path: PathBuf::new(),
            inner: Mutex::new(AppSettings::default()),
        }
    }

    pub fn load() -> Result<Self> {
        let dir = dirs::config_dir()
            .context("could not determine config directory")?
            .join("ALTICR");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("settings.json");

        let settings = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                    tracing::warn!("failed to parse settings.json: {e}; using defaults");
                    AppSettings::default()
                }),
                Err(e) => {
                    tracing::warn!("failed to read settings.json: {e}; using defaults");
                    AppSettings::default()
                }
            }
        } else {
            AppSettings::default()
        };

        Ok(Self {
            path,
            inner: Mutex::new(settings),
        })
    }

    pub fn get(&self) -> AppSettings {
        self.inner.lock().unwrap().clone()
    }

    /// Merge `value` into the current settings and persist.
    pub fn update(&self, value: serde_json::Value) -> Result<AppSettings> {
        let mut current = serde_json::to_value(self.get())?;
        merge_json(&mut current, value);
        let merged: AppSettings = serde_json::from_value(current)?;
        if !self.path.as_os_str().is_empty() {
            let text = serde_json::to_string_pretty(&merged)?;
            std::fs::write(&self.path, text).context("failed to write settings.json")?;
        }
        *self.inner.lock().unwrap() = merged.clone();
        Ok(merged)
    }
}

fn merge_json(base: &mut serde_json::Value, patch: serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(b), serde_json::Value::Object(p)) => {
            for (k, v) in p {
                merge_json(b.entry(k).or_insert(serde_json::Value::Null), v);
            }
        }
        (b, p) => *b = p,
    }
}
