# ALTICR

**Your terminal. Your processes. Your workspace.**

ALTICR is an open-source desktop terminal emulator built with Rust, Tauri 2,
TypeScript, and xterm.js. Every tab owns a real pseudo-terminal (PTY) and an
independent shell process — not a simulator, not captured subprocess output.

## Features

- **Real PTYs**: each tab spawns its own shell through `portable-pty` with
  dedicated reader/waiter threads. Tabs stay alive and keep buffering output
  while hidden.
- **True terminal rendering**: xterm.js frontend + `vt100` backend parser —
  ANSI colors (16/256/truecolor), attributes, cursor control, scroll regions,
  alternate screen, UTF-8 including split sequences, mouse reporting.
- **Dynamic tabs**: tab labels reflect the foreground process via
  `tcgetpgrp`/`ps` inspection (Unix) with OSC title/cwd support.
- **Multiple windows**: independent tab collections per window; new windows
  open centered.
- **Native shortcuts**: Tauri menu accelerators for Cmd/Ctrl+T/N/W, tab
  cycling, font size, find, copy/paste — reliable on macOS WKWebView.
- **Search, copy/paste, scrollback export**: xterm.js search addon, clipboard
  integration, TXT export of scrollback.
- **Persistent settings**: theme, font, cursor, shell, startup dir, env vars,
  scrollback, OSC 52 clipboard policy, tab behavior.
- **Security**: minimal Tauri capabilities, no telemetry, no arbitrary command
  execution from the frontend, OSC 52 clipboard gated by setting.

## Screenshots

<!-- TODO: add screenshots/ before first release -->

## Supported platforms

| OS | Status |
|---|---|
| macOS (Apple Silicon, Intel) | ✅ primary dev target |
| Linux (x86_64, aarch64) | ✅ WebKitGTK 4.1 |
| Windows 10/11 | ✅ WebView2 |

## Install

See [INSTALLATION.md](INSTALLATION.md). Prebuilt binaries are on the
[releases page](https://github.com/live-by-unix/alticr/releases).

## Development

```sh
npm --prefix frontend install
scripts/dev.sh      # frontend watcher + Tauri dev app
scripts/test.sh     # Rust + PTY integration tests
scripts/lint.sh     # fmt, clippy, typecheck
scripts/build.sh    # production bundles
```

See [DEVELOPMENT.md](DEVELOPMENT.md) and [ARCHITECTURE.md](ARCHITECTURE.md).

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — session manager, PTY lifecycle, IPC
- [TERMINAL-COMPATIBILITY.md](TERMINAL-COMPATIBILITY.md) — escape sequence support
- [BUILDING.md](BUILDING.md) — platform build instructions
- [RELEASING.md](RELEASING.md) — release process
- [CONTRIBUTING.md](CONTRIBUTING.md) · [SECURITY.md](SECURITY.md) ·
  [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) · [CHANGELOG.md](CHANGELOG.md)

## License

BSD-3-Clause © 2026 live-by-unix. See [LICENSE](LICENSE).

---

*ALTICR runs real shells with your full privileges. It is a terminal, not a
sandbox.*
