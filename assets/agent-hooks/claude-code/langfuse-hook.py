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
from pathlib import Path


def classify_activity(tool_name: str, tool_input: dict | None) -> str:
    """
    Classify a tool call into an activity kind.

    Returns:
        BUILD: Build/test commands (cargo, pnpm, pytest, etc.)
        CODE: Code modification tools (Edit, Write, NotebookEdit)
        EXPLORE: Read/search/research tools (Read, Glob, Grep, etc.)
    """
    tool_input = tool_input or {}

    # CODE: Direct code modification tools
    code_tools = {"Edit", "Write", "NotebookEdit"}
    if tool_name in code_tools:
        return "CODE"

    # BUILD: Check Bash commands for build/test patterns
    if tool_name == "Bash":
        command = tool_input.get("command", "")
        build_patterns = [
            r"\bcargo\s+(build|test|check|clippy|nextest)",
            r"\bpnpm\s+(build|test|run\s+test)",
            r"\bnpm\s+(run\s+)?(build|test)",
            r"\byarn\s+(build|test)",
            r"\bgo\s+(build|test)",
            r"\bmake\b",
            r"\bdocker\s+build\b",
            r"\bpytest\b",
            r"\bjest\b",
            r"\bvitest\b",
            r"\bcargo-nextest\b",
            r"\bnextest\s+run\b",
        ]
        for pattern in build_patterns:
            if re.search(pattern, command, re.IGNORECASE):
                return "BUILD"

    # EXPLORE: Read/search/research tools
    explore_tools = {
        "Read",
        "Glob",
        "Grep",
        "WebSearch",
        "WebFetch",
        "LSP",
        "Task",
        "LS",
        "ListMcpResourcesTool",
        "ReadMcpResourceTool",
        "MCPSearch",
    }
    if tool_name in explore_tools:
        return "EXPLORE"

    # Default: classify unknown tools as EXPLORE (safer assumption)
    return "EXPLORE"


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
    except ImportError:
        print("Warning: langfuse package not installed", file=sys.stderr)
        return

    public_key = os.environ.get("LANGFUSE_PUBLIC_KEY")
    secret_key = os.environ.get("LANGFUSE_SECRET_KEY")
    host = os.environ.get("LANGFUSE_HOST", "https://cloud.langfuse.com")

    if not public_key or not secret_key:
        print("Warning: LANGFUSE_PUBLIC_KEY or LANGFUSE_SECRET_KEY not set", file=sys.stderr)
        return

    langfuse = Langfuse(public_key=public_key, secret_key=secret_key, host=host)

    # Create a trace for this session
    trace = langfuse.trace(
        name="claude-code-session",
        session_id=session_id,
        metadata={"source": "claude-code", "hook": "langfuse-hook"},
    )

    # Aggregate activity counts
    activity_counts = {"BUILD": 0, "CODE": 0, "EXPLORE": 0}

    for tool_call in tool_calls:
        activity_kind = tool_call["activity_kind"]
        activity_counts[activity_kind] += 1

        # Create a span for each tool call
        trace.span(
            name=tool_call["tool_name"],
            input=tool_call.get("tool_input"),
            metadata={
                "activity_kind": activity_kind,
                "tool_use_id": tool_call.get("tool_use_id"),
            },
        )

    # Update trace with activity summary
    trace.update(
        metadata={
            "source": "claude-code",
            "hook": "langfuse-hook",
            "activity_counts": activity_counts,
            "total_tool_calls": len(tool_calls),
        }
    )

    langfuse.flush()


def main() -> int:
    """Main entry point for the Langfuse hook."""
    # Check if tracing is enabled
    if os.environ.get("TRACE_TO_LANGFUSE", "").lower() != "true":
        return 0

    # Read hook input from stdin
    try:
        hook_input = json.load(sys.stdin)
    except json.JSONDecodeError as e:
        print(f"Error parsing hook input: {e}", file=sys.stderr)
        return 0  # Exit gracefully to not block agent

    session_id = hook_input.get("session_id", "unknown")
    transcript_path = hook_input.get("transcript_path", "")

    if not transcript_path:
        print("Warning: No transcript_path in hook input", file=sys.stderr)
        return 0

    # Parse transcript and extract tool calls
    try:
        tool_calls = parse_transcript(transcript_path)
    except Exception as e:
        print(f"Error parsing transcript: {e}", file=sys.stderr)
        return 0

    if not tool_calls:
        return 0

    # Send to Langfuse
    try:
        send_to_langfuse(session_id, tool_calls)
    except Exception as e:
        print(f"Error sending to Langfuse: {e}", file=sys.stderr)
        return 0

    return 0


if __name__ == "__main__":
    sys.exit(main())
