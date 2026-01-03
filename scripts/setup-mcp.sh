#!/bin/bash
set -euo pipefail

# Configure Claude Code to use the patched vibe-kanban MCP.

echo "Removing existing vibe_kanban MCP configuration..."
claude mcp remove vibe_kanban -s user 2>/dev/null || true

echo "Adding patched vibe-kanban MCP..."
claude mcp add vibe_kanban -s user -- ~/.local/bin/vibe-kanban-mcp-patched

echo "Done! Restart Claude Code for changes to take effect."
echo ""
echo "Patched MCP includes these additional tools:"
echo "  - add_task_dependency"
echo "  - remove_task_dependency"
echo "  - get_task_dependencies"
echo "  - get_task_dependency_tree"
