# Development

## Prerequisites

- Rust stable (1.85+)
- Node.js 20+ and npm
- Tauri CLI v2 (`cargo install tauri-cli --version '^2'` or `npm i -g @tauri-apps/cli`)
- Platform deps: macOS needs Xcode CLT; Linux needs WebKitGTK 4.1 (see BUILDING.md); Windows needs MSVC build tools + WebView2.

## Setup

```sh
npm --prefix frontend install
cd src-tauri && cargo fetch
```

## Running in dev mode

```sh
scripts/dev.sh
# or manually:
npm --prefix frontend run dev &        # serves frontend on :1420 + watches dist/
cd src-tauri && cargo tauri dev        # builds & launches the app
```

The frontend watcher rebuilds `frontend/dist` and writes
`dist/.build-stamp`; the running app polls the stamp and reloads the
webview automatically.

## Testing

```sh
scripts/test.sh   # cargo test --manifest-path src-tauri/Cargo.toml
```

`src-tauri/tests/pty_sessions.rs` launches real shells and covers
session lifecycle, concurrent PTYs, UTF-8/ANSI handling, resize,
process detection, OSC sequences, and shutdown.

## Lint / format

```sh
scripts/lint.sh   # cargo fmt --check, clippy -D warnings, tsc --noEmit
```

## Project layout

```
src-tauri/src/
  main.rs          entry point
  lib.rs           Tauri app setup, native menu, event bridge, polling
  session.rs       PTY session: spawn, reader/waiter threads, resize, write
  manager.rs       SessionManager: sessions map, windows map, settings
  commands.rs      Tauri IPC command handlers
  process_info.rs  foreground process/cwd detection (tcgetpgrp, ps, lsof)
  osc.rs           incremental OSC sequence scanner
  settings.rs      JSON settings store with in-memory fallback
frontend/
  index.html       app shell (titlebar, tabbar, hosts, statusbar, modals)
  src/main.ts      tabs, terminals, events, shortcuts, settings UI
  src/api.ts       typed IPC wrappers + event listeners
  src/styles.css   dark theme
```
