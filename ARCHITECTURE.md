# ALTICR Architecture

## Overview

ALTICR is a Tauri 2 desktop application. The Rust backend owns every
pseudo-terminal and shell process; the web frontend renders each session
with xterm.js and implements the window/tab UI.

```
┌──────────────────────────── Webview (frontend/) ────────────────────────────┐
│  main.ts ── Tab[] { Terminal(xterm), FitAddon, SearchAddon, host div }      │
│     │ onData ──► write_session (base64)                                    │
│     │ ◄── alticr://output {session_id, data:b64}                           │
│     │ ◄── alticr://status {state,title,cwd,fg_process,fg_pid}              │
│     │ ◄── alticr://clipboard (OSC 52, gated by setting)                    │
│     │ ◄── alticr://menu-action (native menu accelerators)                  │
└──────────────────────────────────┬──────────────────────────────────────────┘
                                   │ Tauri IPC (invoke + events)
┌──────────────────────────────────▼──────────────────────────────────────────┐
│  SessionManager (Arc, managed Tauri state)                                  │
│    sessions: HashMap<id, Arc<Session>>   windows: HashMap<label, [ids]>     │
│    SettingsStore (JSON in config dir)                                       │
└──────────────────────────────────┬──────────────────────────────────────────┘
                                   │
              ┌────────────────────┼────────────────────┐
              ▼                    ▼                    ▼
        Session s1            Session s2           Session sN
        ┌─────────────┐      ┌─────────────┐      ┌─────────────┐
        │ portable-pty│      │ portable-pty│      │ portable-pty│
        │  master+slave      │  master+slave      │  master+slave
        │ reader thread ──► OSC scan ──► vt100 ──► emit output    │
        │ waiter thread ──► child.wait() ──► emit exit status    │
        │ writer (Mutex) ◄── write_session                         │
        │ master (Mutex) ◄── resize_session                        │
        └─────────────┘      └─────────────┘      └─────────────┘
```

## PTY lifecycle

1. `create_session` resolves shell (config > `$SHELL` > login shell),
   args, cwd, env, and dimensions, then calls `Session::spawn`.
2. `portable_pty::openpty` allocates the master/slave pair; the slave is
   spawned with the shell via `CommandBuilder` (TERM=xterm-256color,
   COLORTERM=truecolor injected).
3. A **reader thread** blocks on `master.try_clone_reader()`, feeding each
   chunk to the OSC scanner and the `vt100` parser, then emitting raw bytes
   to the frontend as a base64 Tauri event.
4. A **waiter thread** blocks on `child.wait()` and records the exit code
   into the session state.
5. Input goes through `Session::write` → PTY writer. Resizes go through
   `Session::resize` → `master.resize` (kernel sends SIGWINCH to the
   foreground process group).
6. `close_session`/`close_window_sessions` call `killer.kill()`; the
   waiter thread reaps the child. Window `Destroyed` events trigger
   cleanup for that window's sessions.

## Terminal rendering

- **Frontend**: xterm.js renders the real terminal screen (grid, cursor,
  attributes, scrollback, mouse reporting). It is a mature VT emulator —
  not a mock.
- **Backend**: a `vt100` parser consumes the same byte stream for
  scrollback capture (`export_session`) and is exercised by integration
  tests for UTF-8/ANSI correctness.
- All terminal hosts stay mounted (absolutely positioned, toggled by
  visibility) so every session keeps correct dimensions and keeps
  buffering output while hidden.

## Process inspection

Every 2 s the manager polls all running sessions. On Unix it calls
`tcgetpgrp` on the PTY master fd to get the foreground process group,
then maps it to pid/name via one `ps -axo pid,pgid,comm` snapshot.
cwd is refreshed via `/proc/<pid>/cwd` (Linux) or `lsof` (other Unix).
OSC 7 sequences update cwd directly. Windows falls back to the shell name.

## IPC surface

Commands (invoke): `create_session`, `write_session`, `resize_session`,
`close_session`, `list_sessions`, `export_session`, `get_settings`,
`set_settings`, `detect_shells`, `close_window_sessions`, `focus_window`.

Events (backend → frontend): `alticr://output`, `alticr://status`,
`alticr://clipboard`, `alticr://menu-action`.

PTY output is base64-encoded inside JSON events — simple, robust, and
independent of Tauri's channel serialization details.

## Settings

`settings.json` in the platform config dir (`dirs::config_dir()/ALTICR`).
`set_settings` deep-merges a JSON patch and rewrites the file atomically
enough for a desktop app (write-then-rename is a future improvement).

## Security model

- Capabilities (`capabilities/default.json`) grant only the window,
  event, path, and opener permissions the UI actually uses.
- The frontend cannot spawn arbitrary processes — the only process
  creation path is `create_session` through the session manager.
- OSC 52 clipboard writes are ignored unless `allow_osc52_clipboard` is
  enabled in settings.
- No telemetry, no network calls, no remote code execution.
- ALTICR runs shells with the user's full privileges — it is a terminal,
  not a sandbox. See SECURITY.md.
