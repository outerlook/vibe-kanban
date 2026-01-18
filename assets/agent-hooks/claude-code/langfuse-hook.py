#!/usr/bin/env python3
"""
Langfuse hook for Claude Code activity monitoring.

This script is invoked as a Stop hook by Claude Code. It reads the session transcript,
extracts tool calls, classifies them into activity kinds (BUILD/CODE/EXPLORE),
and sends traces to Langfuse.

Environment variables:
    TRACE_TO_LANGFUSE: Set to "true" to enable tracing (required)
    LANGFUSE_PUBLIC_KEY: Langfuse public key (required when tracing)
    LANGFUSE_SECRET_KEY: Langfuse secret key (required when tracing)
    LANGFUSE_HOST: Langfuse host URL (optional, defaults to https://cloud.langfuse.com)
"""

import json
import os
import re
import sys
from datetime import datetime
from pathlib import Path


def debug_log(message: str) -> None:
    """Write debug message to a file for troubleshooting."""
    debug_file = Path.home() / ".vibe-kanban" / "langfuse-hook-debug.log"
    debug_file.parent.mkdir(parents=True, exist_ok=True)
    with open(debug_file, "a", encoding="utf-8") as f:
        f.write(f"[{datetime.now().isoformat()}] {message}\n")


def classify_activity(tool_name: str, tool_input: dict | None) -> str:
    """
    Classify a tool call into an activity kind.

    Returns:
        CODE: Code modification tools (Edit, Write, NotebookEdit)
        BUILD: Compile, lint, typecheck, format commands
        TEST: Test execution commands (pytest, jest, cargo test, etc.)
        GIT: Git version control operations
        EXPLORE: Read/search in codebase (Read, Glob, Grep, LSP)
        RESEARCH: Web search and external documentation (WebSearch, WebFetch)
        SETUP: Dependency installation and environment setup
        PLAN: Planning and task management (TodoWrite, EnterPlanMode)
        COMMUNICATE: User interaction (AskUserQuestion)
    """
    tool_input = tool_input or {}

    # CODE: Direct code modification tools
    if tool_name in {"Edit", "Write", "NotebookEdit"}:
        return "CODE"

    # PLAN: Planning and task management
    if tool_name in {"TodoWrite", "EnterPlanMode", "ExitPlanMode"}:
        return "PLAN"

    # COMMUNICATE: User interaction
    if tool_name in {"AskUserQuestion"}:
        return "COMMUNICATE"

    # RESEARCH: Web search and external documentation
    if tool_name in {"WebSearch", "WebFetch"}:
        return "RESEARCH"

    # EXPLORE: Read/search in codebase
    if tool_name in {"Read", "Glob", "Grep", "LSP", "LS", "Task", "ListMcpResourcesTool", "ReadMcpResourceTool"}:
        return "EXPLORE"

    # Bash command classification
    if tool_name == "Bash":
        command = tool_input.get("command", "")

        # GIT: Version control operations
        git_patterns = [
            r"\bgit\s+(status|diff|log|show|branch|checkout|merge|rebase|pull|fetch|clone|add|commit|push|stash|reset|cherry-pick)",
            r"\bgh\s+",  # GitHub CLI
        ]
        for pattern in git_patterns:
            if re.search(pattern, command, re.IGNORECASE):
                return "GIT"

        # TEST: Test execution commands (check BEFORE setup to catch uvx pytest, etc.)
        test_patterns = [
            # Rust - nextest variations
            r"\bcargo\s+nextest\b",
            r"\bcargo-nextest\b",
            r"\bnextest\s+(run|list)\b",
            r"\bcargo\s+test\b",
            # JavaScript/TypeScript - explicit test commands
            r"\b(pnpm|npm|yarn|bun)\s+(run\s+)?test\b",
            r"\b(pnpm|npm|yarn|bun)\s+exec\s+(jest|vitest|mocha)\b",
            r"\bnpx\s+(jest|vitest|mocha|ava|playwright)\b",
            r"^\s*jest\s",  # jest at start
            r"&&\s*jest\s",  # jest after &&
            r"\bjest\s+--",  # jest with flags
            r"\bvitest(\s|$)",  # vitest with args or alone
            r"\bmocha\s",
            r"\bava\s",
            r"\bplaywright\s+test\b",
            r"\bcypress\s+(run|open)\b",
            # Python
            r"\bpytest\b",
            r"\bpython\s+-m\s+(pytest|unittest)\b",
            r"\buvx\s+pytest\b",
            # Go
            r"\bgo\s+test\b",
        ]
        for pattern in test_patterns:
            if re.search(pattern, command, re.IGNORECASE):
                return "TEST"

        # BUILD: Compile, lint, typecheck, format commands (check BEFORE setup)
        build_patterns = [
            # Rust
            r"\bcargo\s+(build|check|clippy|fmt|bench|doc)\b",
            r"\brustfmt\b",
            # JavaScript/TypeScript
            r"\b(pnpm|npm|yarn|bun)\s+(run\s+)?(build|check|lint|typecheck|format|prettier|eslint)\b",
            r"\b(pnpm|npm|yarn|bun)\s+(build|check|lint)\b",
            r"\bnpx\s+(tsc|eslint|prettier|biome)\b",
            r"\btsc(\s|$)",  # tsc with args or alone
            r"\beslint\s",
            r"\bprettier\s",
            r"\bbiome\s+(check|lint|format)\b",
            # Python
            r"\bpython\s+-m\s+(mypy|ruff|black|flake8|isort)\b",
            r"\bmypy\s",
            r"\bruff\s+(check|format)\b",
            r"\bblack\s",
            r"\bflake8\s",
            r"\buvx\s+(mypy|ruff|black|flake8)\b",
            r"\bisort\s",
            # Go
            r"\bgo\s+(build|vet|fmt|generate)\b",
            r"\bgolangci-lint\b",
            # Make - various forms
            r"^\s*make(\s|$)",  # make at start, with args or alone
            r"&&\s*make(\s|$)",  # make after &&
            r"\bmake\s+(build|test|check|lint|all|clean)\b",  # make with common targets
            r"\bdocker\s+(build|compose)\b",
        ]
        for pattern in build_patterns:
            if re.search(pattern, command, re.IGNORECASE):
                return "BUILD"

        # SETUP: Dependency installation and environment setup (check AFTER test/build)
        setup_patterns = [
            r"\b(pnpm|npm|yarn|bun)\s+(install|add|remove|update|upgrade|ci)\b",
            r"\b(pip|uv)\s+install\b",
            r"\buvx\s+\S+",  # uvx running any tool (generic, after specific uvx patterns)
            r"\bcargo\s+(add|remove|update)\b",
            r"\bgo\s+(get|mod\s+(download|tidy))\b",
            r"\bdocker\s+(pull|run|start|stop|rm|exec)\b",
            r"^\s*(chmod|mkdir|cp|mv)\s",  # Only at start of command/subcommand
            r"&&\s*(chmod|mkdir|cp|mv)\s",  # Or after &&
        ]
        for pattern in setup_patterns:
            if re.search(pattern, command, re.IGNORECASE):
                return "SETUP"

    # Default: unclassified bash commands and unknown tools
    return "OTHER"


def parse_transcript(transcript_path: str) -> list[dict]:
    """
    Parse the Claude Code transcript JSONL file and extract tool calls.

    Returns a list of tool call records with:
        - tool_name: Name of the tool
        - tool_input: Input parameters
        - activity_kind: Classified activity (BUILD/CODE/EXPLORE)
        - timestamp: When the tool was called (if available)
    """
    tool_calls = []
    path = Path(transcript_path.replace("~", str(Path.home())))

    if not path.exists():
        return tool_calls

    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue

            try:
                entry = json.loads(line)
            except json.JSONDecodeError:
                continue

            # Extract tool calls from assistant messages
            if entry.get("type") == "assistant":
                message = entry.get("message", {})
                content = message.get("content", [])

                for block in content:
                    if block.get("type") == "tool_use":
                        tool_name = block.get("name", "")
                        tool_input = block.get("input", {})
                        activity_kind = classify_activity(tool_name, tool_input)

                        tool_calls.append(
                            {
                                "tool_name": tool_name,
                                "tool_input": tool_input,
                                "tool_use_id": block.get("id"),
                                "activity_kind": activity_kind,
                            }
                        )

    return tool_calls


def send_to_langfuse(session_id: str, tool_calls: list[dict]) -> None:
    """Send tool call traces to Langfuse."""
    try:
        from langfuse import Langfuse
    except ImportError as e:
        debug_log(f"langfuse package not installed: {e}")
        print("Warning: langfuse package not installed", file=sys.stderr)
        return

    public_key = os.environ.get("LANGFUSE_PUBLIC_KEY")
    secret_key = os.environ.get("LANGFUSE_SECRET_KEY")
    host = os.environ.get("LANGFUSE_HOST", "https://cloud.langfuse.com")

    if not public_key or not secret_key:
        print("Warning: LANGFUSE_PUBLIC_KEY or LANGFUSE_SECRET_KEY not set", file=sys.stderr)
        return

    langfuse = Langfuse(public_key=public_key, secret_key=secret_key, host=host)

    # Aggregate activity counts
    activity_counts = {
        "CODE": 0,
        "BUILD": 0,
        "TEST": 0,
        "GIT": 0,
        "EXPLORE": 0,
        "RESEARCH": 0,
        "SETUP": 0,
        "PLAN": 0,
        "COMMUNICATE": 0,
        "OTHER": 0,
    }

    # Create a trace with a root span using the v3 API
    with langfuse.start_as_current_span(name="claude-code-session") as root_span:
        # Set trace-level properties
        root_span.update_trace(
            name="claude-code-session",
            session_id=session_id,
            metadata={"source": "claude-code", "hook": "langfuse-hook"},
        )

        for tool_call in tool_calls:
            activity_kind = tool_call["activity_kind"]
            activity_counts[activity_kind] += 1

            # Create a child span for each tool call
            child_span = root_span.start_span(
                name=tool_call["tool_name"],
                input=tool_call.get("tool_input"),
                metadata={
                    "activity_kind": activity_kind,
                    "tool_use_id": tool_call.get("tool_use_id"),
                },
            )
            child_span.end()

        # Update root span with activity summary
        root_span.update(
            metadata={
                "activity_counts": activity_counts,
                "total_tool_calls": len(tool_calls),
            }
        )

    langfuse.flush()


def main() -> int:
    """Main entry point for the Langfuse hook."""
    debug_log("Hook started")
    debug_log(f"TRACE_TO_LANGFUSE={os.environ.get('TRACE_TO_LANGFUSE', '<not set>')}")
    debug_log(f"LANGFUSE_PUBLIC_KEY={os.environ.get('LANGFUSE_PUBLIC_KEY', '<not set>')[:20] if os.environ.get('LANGFUSE_PUBLIC_KEY') else '<not set>'}...")
    debug_log(f"LANGFUSE_HOST={os.environ.get('LANGFUSE_HOST', '<not set>')}")

    # Check if tracing is enabled
    if os.environ.get("TRACE_TO_LANGFUSE", "").lower() != "true":
        debug_log("Tracing not enabled, exiting")
        return 0

    # Read hook input from stdin
    try:
        hook_input = json.load(sys.stdin)
        debug_log(f"Hook input: {hook_input}")
    except json.JSONDecodeError as e:
        debug_log(f"Error parsing hook input: {e}")
        print(f"Error parsing hook input: {e}", file=sys.stderr)
        return 0  # Exit gracefully to not block agent

    session_id = hook_input.get("session_id", "unknown")
    transcript_path = hook_input.get("transcript_path", "")
    debug_log(f"session_id={session_id}, transcript_path={transcript_path}")

    if not transcript_path:
        debug_log("No transcript_path in hook input")
        print("Warning: No transcript_path in hook input", file=sys.stderr)
        return 0

    # Parse transcript and extract tool calls
    try:
        tool_calls = parse_transcript(transcript_path)
        debug_log(f"Parsed {len(tool_calls)} tool calls")
    except Exception as e:
        debug_log(f"Error parsing transcript: {e}")
        print(f"Error parsing transcript: {e}", file=sys.stderr)
        return 0

    if not tool_calls:
        debug_log("No tool calls found, exiting")
        return 0

    # Send to Langfuse
    try:
        debug_log("Sending to Langfuse...")
        send_to_langfuse(session_id, tool_calls)
        debug_log("Successfully sent to Langfuse")
    except Exception as e:
        debug_log(f"Error sending to Langfuse: {e}")
        print(f"Error sending to Langfuse: {e}", file=sys.stderr)
        return 0

    return 0


if __name__ == "__main__":
    sys.exit(main())
