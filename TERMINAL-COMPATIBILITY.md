# Terminal Compatibility

ALTICR uses two complementary terminal layers:

## Rendering (frontend)

xterm.js provides the on-screen terminal: grid rendering, cursor,
attributes (bold/dim/italic/underline/strikethrough/inverse), 16/256/24-bit
colors, scrollback, selection, mouse reporting, and bracketed paste.

## State & parsing (backend)

A `vt100` parser consumes the same byte stream for scrollback capture
(`export_session`) and is covered by integration tests.

## Protocol support matrix

| Feature | Support | Notes |
|---|---|---|
| SGR colors (16/256/truecolor) | ✅ | Via xterm.js + vt100 |
| Text attributes | ✅ | bold, dim, italic, underline, strike, inverse |
| Cursor movement/visibility | ✅ | incl. application cursor keys |
| Screen/line clearing, scroll regions | ✅ | alternate screen supported by vt100 |
| UTF-8 incl. split sequences | ✅ | Tested across PTY read boundaries |
| OSC 0/2 window title | ✅ | Updates tab/window titles |
| OSC 7 cwd reporting | ✅ | Updates status bar cwd |
| OSC 52 clipboard | ⚙️ | Disabled by default; enable in settings |
| Bracketed paste | ✅ | Tauri clipboard → PTY input |
| Mouse tracking | ✅ | xterm.js mouse protocol |
| Terminal resize (SIGWINCH) | ✅ | `master.resize` notifies fg process group |
| DCS/APC/SOS/PM | ✅ | vt100 parses; ignored for display |
| DECRQSS/XTGETTCAP | ⚠️ | Parsed, replies not synthesized |

Anything the frontend renderer supports but the backend vt100 parser does
not is still displayed correctly on screen; only scrollback export and
process detection are affected.
