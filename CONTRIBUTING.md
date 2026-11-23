# Contributing to ALTICR

Thanks for your interest. ALTICR is a real terminal emulator — correctness
of the PTY/session layer matters more than UI polish.

## Ground rules

- Every tab owns an independent PTY. Never kill/restart a session on tab
  switch, reorder, or hide. Output must keep buffering while hidden.
- The frontend must never spawn processes directly; all process creation
  goes through the session manager.
- PTY output is binary-safe: it travels base64-encoded over Tauri events.
  Do not assume UTF-8 boundaries align with reads.
- New dependencies need a reason and a license check.

## Development setup

```sh
npm --prefix frontend install
cargo install tauri-cli --version '^2'   # or use npx @tauri-apps/cli
scripts/dev.sh                            # starts frontend watcher + app
```

## Before submitting

```sh
scripts/lint.sh    # cargo fmt --check + clippy -D warnings + tsc
scripts/test.sh    # cargo test (real PTY integration tests)
scripts/build.sh   # production build
```

- Rust: `cargo fmt`, `cargo clippy -- -D warnings`, tests must pass.
- Frontend: `npm --prefix frontend run typecheck` and `npm run build`.
- Integration tests launch real shells — keep them deterministic (no
  reliance on interactive prompt configuration; use `--noprofile`-style
  flags or strip OSC sequences when asserting).

## Commit style

Short imperative subject, body explains *why*. Sign-off appreciated but
not required.
