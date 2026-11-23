#!/usr/bin/env bash
# Production build for the host platform.
set -euo pipefail
cd "$(dirname "$0")/.."
npm --prefix frontend run build
cargo tauri build --manifest-path src-tauri/Cargo.toml "$@"
