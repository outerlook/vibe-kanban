#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PATCHED_BIN="$REPO_ROOT/patched-bin"
LOCAL_BIN="$HOME/.local/bin"

cd "$REPO_ROOT"

echo "==> Building frontend..."
cd frontend
pnpm install --frozen-lockfile
pnpm run build
cd "$REPO_ROOT"

echo "==> Building Rust binaries (nightly)..."
RUSTUP_TOOLCHAIN=nightly-2025-05-01 cargo build --release -p server

echo "==> Copying to patched-bin..."
mkdir -p "$PATCHED_BIN"
cp target/release/server "$PATCHED_BIN/vibe-kanban"
cp target/release/mcp_task_server "$PATCHED_BIN/vibe-kanban-mcp"

echo "==> Ensuring symlinks in ~/.local/bin..."
mkdir -p "$LOCAL_BIN"
ln -sf "$PATCHED_BIN/vibe-kanban" "$LOCAL_BIN/vibe-kanban-patched" || true
ln -sf "$PATCHED_BIN/vibe-kanban-mcp" "$LOCAL_BIN/vibe-kanban-mcp-patched" || true

echo "==> Done!"
echo "Binaries: $PATCHED_BIN/"
echo "Symlinks: $LOCAL_BIN/vibe-kanban-patched, $LOCAL_BIN/vibe-kanban-mcp-patched"
