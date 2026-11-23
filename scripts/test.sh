#!/usr/bin/env bash
# Run the full test suite: Rust unit + integration tests, frontend typecheck.
set -euo pipefail
cd "$(dirname "$0")/.."
echo "== Rust tests =="
cargo test --manifest-path src-tauri/Cargo.toml
echo "== Frontend typecheck =="
npm --prefix frontend run typecheck
echo "== All tests passed =="
