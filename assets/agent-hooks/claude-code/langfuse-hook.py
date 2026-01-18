#!/usr/bin/env python3
"""
Langfuse hook for Claude Code activity monitoring.

This script is invoked as a Stop hook by Claude Code. It reads the session transcript,
extracts the complete conversation structure including user messages, assistant responses,
tool calls, and token usage. Tool calls are classified into activity kinds (BUILD/CODE/EXPLORE),
and traces are sent to Langfuse.

Environment variables:
    TRACE_TO_LANGFUSE: Set to "true" to enable tracing (required)
    LANGFUSE_PUBLIC_KEY: Langfuse public key (required when tracing)
    LANGFUSE_SECRET_KEY: Langfuse secret key (required when tracing)
    LANGFUSE_HOST: Langfuse host URL (optional, defaults to https://cloud.langfuse.com)
    DEBUG_LANGFUSE_HOOK: Set to "true" to print parsed transcript to stderr

Vibe-Kanban context (optional, set when running in vibe-kanban workspace):
    VK_TASK_ID: The kanban task being worked on
    VK_ATTEMPT_ID: The specific execution attempt
    VK_WORKSPACE_ID: The workspace/worktree ID
"""

import json
import os
import re
import sys
from pathlib import Path


def get_vk_context() -> dict[str, str | None]:
    """
    Extract vibe-kanban context from environment variables.

    Returns a dict with:
        - vk_task_id: The kanban task being worked on (or None)
        - vk_attempt_id: The specific execution attempt (or None)
        - vk_workspace_id: The workspace/worktree ID (or None)

    These environment variables are set by vibe-kanban when running agents
    in a workspace context (see crates/local-deployment/src/container.rs).
    """
    context = {
        "vk_task_id": os.environ.get("VK_TASK_ID"),
        "vk_attempt_id": os.environ.get("VK_ATTEMPT_ID"),
        "vk_workspace_id": os.environ.get("VK_WORKSPACE_ID"),
    }

    # Debug logging for extracted context
    non_empty = {k: v for k, v in context.items() if v is not None}
    if non_empty:
        print(f"Debug: VK context extracted: {non_empty}", file=sys.stderr)
    else:
        print("Debug: No VK context found in environment", file=sys.stderr)

    return context


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


def parse_transcript(transcript_path: str) -> dict:
    """
    Parse the Claude Code transcript JSONL file and extract complete conversation structure.

    Returns a dict with:
        - session_metadata: {cwd, git_branch, model}
        - turns: List of {user_message, assistant_response} pairs
        - totals: {input_tokens, output_tokens, activity_counts}
    """
    result = {
        "session_metadata": {"cwd": None, "git_branch": None, "model": None},
        "turns": [],
        "totals": {
            "input_tokens": 0,
            "output_tokens": 0,
            "cache_read_input_tokens": 0,
            "cache_creation_input_tokens": 0,
            "activity_counts": {"BUILD": 0, "CODE": 0, "EXPLORE": 0},
        },
    }

    path = Path(transcript_path.replace("~", str(Path.home())))
    if not path.exists():
        return result

    pending_user_message: str | None = None

    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue

            try:
                entry = json.loads(line)
            except json.JSONDecodeError:
                continue

            entry_type = entry.get("type")

            # Extract session metadata from various sources
            if entry_type == "summary" and result["session_metadata"]["cwd"] is None:
                result["session_metadata"]["cwd"] = entry.get("cwd")
                result["session_metadata"]["git_branch"] = entry.get("git_branch")

            # Parse user messages
            elif entry_type == "user":
                message = entry.get("message", {})
                content = message.get("content", [])
                text_parts = []
                for block in content:
                    if isinstance(block, str):
                        text_parts.append(block)
                    elif isinstance(block, dict) and block.get("type") == "text":
                        text_parts.append(block.get("text", ""))
                pending_user_message = "\n".join(text_parts) if text_parts else None

            # Parse assistant messages
            elif entry_type == "assistant":
                message = entry.get("message", {})
                content = message.get("content", [])
                usage = message.get("usage", {})
                model = message.get("model")

                # Set model in metadata if not already set
                if model and result["session_metadata"]["model"] is None:
                    result["session_metadata"]["model"] = model

                # Extract text content (skip thinking blocks)
                text_parts = []
                tool_calls = []

                for block in content:
                    block_type = block.get("type")

                    if block_type == "text":
                        text_parts.append(block.get("text", ""))

                    elif block_type == "tool_use":
                        tool_name = block.get("name", "")
                        tool_input = block.get("input", {})
                        activity_kind = classify_activity(tool_name, tool_input)

                        # Update activity counts
                        result["totals"]["activity_counts"][activity_kind] += 1

                        tool_calls.append({
                            "tool_name": tool_name,
                            "tool_input": tool_input,
                            "tool_use_id": block.get("id"),
                            "activity_kind": activity_kind,
                        })

                # Extract token usage
                assistant_usage = {
                    "input_tokens": usage.get("input_tokens", 0),
                    "output_tokens": usage.get("output_tokens", 0),
                    "cache_read_input_tokens": usage.get("cache_read_input_tokens", 0),
                    "cache_creation_input_tokens": usage.get("cache_creation_input_tokens", 0),
                }

                # Aggregate totals
                result["totals"]["input_tokens"] += assistant_usage["input_tokens"]
                result["totals"]["output_tokens"] += assistant_usage["output_tokens"]
                result["totals"]["cache_read_input_tokens"] += assistant_usage["cache_read_input_tokens"]
                result["totals"]["cache_creation_input_tokens"] += assistant_usage["cache_creation_input_tokens"]

                # Create turn entry
                turn = {
                    "user_message": pending_user_message,
                    "assistant_response": {
                        "model": model,
                        "text_content": "\n".join(text_parts) if text_parts else None,
                        "usage": assistant_usage,
                        "tool_calls": tool_calls,
                    },
                }
                result["turns"].append(turn)
                pending_user_message = None

    return result


def truncate_text(text: str | None, max_len: int = 500) -> str | None:
    """Truncate text to max_len characters, adding ellipsis if truncated."""
    if text is None:
        return None
    if len(text) <= max_len:
        return text
    return text[:max_len] + "..."


def send_to_langfuse(session_id: str, parsed: dict, vk_context: dict[str, str | None]) -> None:
    """
    Send parsed transcript traces to Langfuse with hierarchical structure.

    Creates:
        Trace: claude-code-session
        ├── session_id, metadata (vk_*, model, git_branch, activity_counts, totals)
        ├── input: first user message (truncated to 500 chars)
        ├── output: last assistant text (truncated to 500 chars)
        │
        ├── Generation Span: "llm-response-{i}"
        │   ├── model, input (user message), output (assistant text)
        │   ├── usage: {input, output, cache_read, cache_creation}
        │   │
        │   └── Tool Span: "{tool_name}" (child for each tool_call)
        │       ├── input: tool_input dict
        │       └── metadata: {activity_kind, tool_use_id}
    """
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

    turns = parsed.get("turns", [])
    totals = parsed.get("totals", {})
    session_metadata = parsed.get("session_metadata", {})

    # Extract first user message and last assistant text for trace-level I/O
    first_user_message = None
    last_assistant_text = None
    for turn in turns:
        user_msg = turn.get("user_message")
        if user_msg and first_user_message is None:
            first_user_message = user_msg
        assistant_text = turn.get("assistant_response", {}).get("text_content")
        if assistant_text:
            last_assistant_text = assistant_text

    # Count total tool calls
    total_tool_calls = sum(
        len(turn.get("assistant_response", {}).get("tool_calls", []))
        for turn in turns
    )

    # Build trace metadata including VK context
    trace_metadata = {
        "source": "claude-code",
        "hook": "langfuse-hook",
        "model": session_metadata.get("model"),
        "git_branch": session_metadata.get("git_branch"),
        "cwd": session_metadata.get("cwd"),
        "activity_counts": totals.get("activity_counts", {}),
        "total_tool_calls": total_tool_calls,
        "token_totals": {
            "input_tokens": totals.get("input_tokens", 0),
            "output_tokens": totals.get("output_tokens", 0),
            "cache_read_input_tokens": totals.get("cache_read_input_tokens", 0),
            "cache_creation_input_tokens": totals.get("cache_creation_input_tokens", 0),
        },
    }
    # Add VK context fields (only non-None values)
    for key, value in vk_context.items():
        if value is not None:
            trace_metadata[key] = value

    # Create trace with root span
    with langfuse.start_as_current_span(name="claude-code-session") as root_span:
        root_span.update_trace(
            name="claude-code-session",
            session_id=session_id,
            input=truncate_text(first_user_message),
            output=truncate_text(last_assistant_text),
            metadata=trace_metadata,
        )

        # Create generation span for each turn with tool spans as children
        for i, turn in enumerate(turns):
            user_message = turn.get("user_message")
            assistant_response = turn.get("assistant_response", {})
            model = assistant_response.get("model")
            text_content = assistant_response.get("text_content")
            usage = assistant_response.get("usage", {})
            tool_calls = assistant_response.get("tool_calls", [])

            # Create generation span for this LLM response
            gen_span = root_span.start_span(
                name=f"llm-response-{i}",
                input=truncate_text(user_message),
                output=truncate_text(text_content),
                metadata={
                    "model": model,
                    "tool_call_count": len(tool_calls),
                },
            )

            # Update with usage details
            gen_span.update(
                usage_details={
                    "input": usage.get("input_tokens", 0),
                    "output": usage.get("output_tokens", 0),
                    "cache_read": usage.get("cache_read_input_tokens", 0),
                    "cache_creation": usage.get("cache_creation_input_tokens", 0),
                },
            )

            # Create tool spans as children of the generation span
            for tool_call in tool_calls:
                tool_span = gen_span.start_span(
                    name=tool_call["tool_name"],
                    input=tool_call.get("tool_input"),
                    metadata={
                        "activity_kind": tool_call["activity_kind"],
                        "tool_use_id": tool_call.get("tool_use_id"),
                    },
                )
                tool_span.end()

            gen_span.end()

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

    # Parse transcript
    try:
        parsed = parse_transcript(transcript_path)
    except Exception as e:
        print(f"Error parsing transcript: {e}", file=sys.stderr)
        return 0

    # Extract VK context from environment
    vk_context = get_vk_context()

    # Calculate stats for logging
    turns = parsed.get("turns", [])
    totals = parsed.get("totals", {})
    turn_count = len(turns)
    total_tool_calls = sum(
        len(turn.get("assistant_response", {}).get("tool_calls", []))
        for turn in turns
    )

    # Debug logging
    if os.environ.get("DEBUG_LANGFUSE_HOOK", "").lower() == "true":
        print(f"Parsed transcript: {json.dumps(parsed, indent=2, default=str)}", file=sys.stderr)

    print(
        f"Langfuse hook: {turn_count} turns, {total_tool_calls} tool calls, "
        f"{totals.get('input_tokens', 0)} input tokens, {totals.get('output_tokens', 0)} output tokens",
        file=sys.stderr,
    )

    # Send to Langfuse
    try:
        send_to_langfuse(session_id, parsed, vk_context)
    except Exception as e:
        print(f"Error sending to Langfuse: {e}", file=sys.stderr)
        return 0

    return 0


if __name__ == "__main__":
    sys.exit(main())
