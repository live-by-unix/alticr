# Changelog

All notable changes to ALTICR are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- Real PTY backend: every tab owns an independent `portable-pty` session
  with dedicated reader/waiter threads and a `vt100` parser.
- xterm.js frontend with fit, search, and web-links addons; hidden tabs
  keep their PTYs alive and buffering output.
- Multi-window support, native menu accelerators, in-app dialogs.
- Foreground process detection via `tcgetpgrp` + `ps` snapshot (Unix).
- OSC title/cwd/clipboard scanning; OSC 52 clipboard gated by setting.
- Persistent JSON settings; settings UI with Appearance/Terminal/Tabs/
  Keyboard/Advanced sections.
- Integration tests launching real shells (creation, concurrency, UTF-8,
  ANSI, resize, exit, process detection, OSC).

### Fixed
- Split `ESC \` OSC terminators across PTY reads.
- Settings persistence when config dir does not exist.
- Session snapshot dimensions not updated on resize.
- New-tab popover clamped inside the viewport.
- `window.confirm()` replaced with in-app dialog (unreliable in webview).
