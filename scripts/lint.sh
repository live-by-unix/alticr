#!/usr/bin/env bash
# Formatting + lint.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm --prefix frontend run typecheck
echo "Lint clean."
