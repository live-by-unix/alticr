# Security Policy

ALTICR is a terminal emulator: it runs shells with **your full user
privileges**. It is not a sandbox.

## Reporting a vulnerability

Please do not open a public issue. Email the maintainer via the GitHub
profile contact, or open a private security advisory on the repository.
Include reproduction steps and impact.

## Threat model notes

- The Tauri capability set (`src-tauri/capabilities/default.json`) is
  intentionally minimal: window, event, path, and opener permissions only.
- The frontend cannot execute arbitrary commands; the sole process
  creation path is `create_session` via the session manager, which only
  spawns the configured shell.
- OSC 52 clipboard writes are disabled by default and gated behind the
  `allow_osc52_clipboard` setting, since any program running in a
  terminal could otherwise overwrite the clipboard.
- Terminal output is treated as data, never as HTML/JS.
- No telemetry, analytics, or network access is performed by the app.
- PTY output travels over Tauri's local IPC only.

If you find a way for untrusted terminal output to escape the webview
context or for the frontend to invoke unintended capabilities, that is
in scope for this policy.
