#!/usr/bin/env bash
# ALTICR development runner: builds the frontend watcher + launches the app.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo tauri dev --manifest-path src-tauri/Cargo.toml "$@"
