# Patched vibe-kanban Fork

Local fork of [BloopAI/vibe-kanban](https://github.com/BloopAI/vibe-kanban) with SQLite concurrency improvements in `crates/db/src/lib.rs`.

## Patched Binaries

Located in `./patched-bin/`:
- `vibe-kanban` - Main UI server (serves the frontend)
- `vibe-kanban-mcp` - MCP server for Claude Code integration

Symlinked to `~/.local/bin/`:
- `vibe-kanban-patched`
- `vibe-kanban-mcp-patched`

## How to Rebuild

```bash
cd ~/random-codes/vibe-kanban-fork

# 1. Build frontend
cd frontend && pnpm install && pnpm run build && cd ..

# 2. Build binaries (requires nightly)
RUSTUP_TOOLCHAIN=nightly-2025-05-01 cargo build --release -p server

# 3. Copy to patched-bin
cp target/release/server patched-bin/vibe-kanban
cp target/release/mcp_task_server patched-bin/vibe-kanban-mcp
```

## MCP Configuration

Claude Code defaults to the npm MCP (`npm exec vibe-kanban@latest --mcp`), which
does not include dependency tools. Use the patched MCP to get the extra tools.

```bash
# One-time setup
./scripts/setup-mcp.sh

# Or manually:
claude mcp remove vibe_kanban -s user
claude mcp add vibe_kanban -s user -- ~/.local/bin/vibe-kanban-mcp-patched

# Verify which MCP is configured
claude mcp list | grep vibe_kanban
```

Restart Claude Code for changes to take effect.

## Running

```bash
~/.local/bin/vibe-kanban-patched
```

## Syncing with Upstream

```bash
git fetch origin
git merge origin/main
# Keep modifications in crates/db/src/lib.rs
# Then rebuild
```
