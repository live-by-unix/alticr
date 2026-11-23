#!/usr/bin/env bash
# Remove all build artifacts.
set -euo pipefail
cd "$(dirname "$0")/.."
rm -rf src-tauri/target frontend/dist frontend/node_modules/.cache
cargo clean --manifest-path src-tauri/Cargo.toml 2>/dev/null || true
echo "Cleaned."
