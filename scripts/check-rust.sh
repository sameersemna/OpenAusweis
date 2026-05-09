#!/usr/bin/env bash
set -euo pipefail

cargo fmt --all --check
cargo clippy --workspace --exclude openausweis-desktop --all-targets -- -D warnings
cargo check --workspace --exclude openausweis-desktop
