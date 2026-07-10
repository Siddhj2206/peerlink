#!/usr/bin/env sh
# Hot-reload development loop.
# Requires: cargo-watch (install with: cargo install cargo-watch)
set -eu

if ! command -v cargo-watch >/dev/null 2>&1; then
    echo "Installing cargo-watch for hot-reload..."
    cargo install cargo-watch
fi

exec cargo watch -x run
