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
    VK_PROJECT_ID: The project UUID
    VK_PROJECT_NAME: The project name
    VK_TASK_ID: The kanban task being worked on
    VK_ATTEMPT_ID: The specific execution attempt
    VK_WORKSPACE_ID: The workspace/worktree ID
    VK_WORKSPACE_BRANCH: The git branch for the workspace
    VK_EXECUTION_PURPOSE: The purpose of execution (e.g., "task", "feedback")
    VK_REPO_NAMES: Comma-separated list of repository names in the workspace
"""

import hashlib
import json
import os
import re
import sys
import uuid
from datetime import datetime, timezone
from pathlib import Path


def debug_log(message: str) -> None:
    """Write debug message to a file for troubleshooting."""
    debug_file = Path.home() / ".vibe-kanban" / "langfuse-hook-debug.log"
    debug_file.parent.mkdir(parents=True, exist_ok=True)
    with open(debug_file, "a", encoding="utf-8") as f:
        f.write(f"[{datetime.now().isoformat()}] {message}\n")


def get_vk_context() -> dict[str, str | None]:
    """
    Extract vibe-kanban context from environment variables.

    Returns a dict with:
        - vk_project_id: The project ID (or None)
        - vk_project_name: The project name (or None)
        - vk_task_id: The kanban task being worked on (or None)
        - vk_attempt_id: The specific execution attempt (or None)
        - vk_workspace_id: The workspace/worktree ID (or None)
        - vk_workspace_branch: The git branch for the workspace (or None)
        - vk_execution_purpose: The purpose of execution (or None)
        - vk_repo_names: Comma-separated list of repo names (or None)

    These environment variables are set by vibe-kanban when running agents
    in a workspace context (see crates/local-deployment/src/container.rs).
    """
    return {
        "vk_project_id": os.environ.get("VK_PROJECT_ID"),
        "vk_project_name": os.environ.get("VK_PROJECT_NAME"),
        "vk_task_id": os.environ.get("VK_TASK_ID"),
        "vk_attempt_id": os.environ.get("VK_ATTEMPT_ID"),
        "vk_workspace_id": os.environ.get("VK_WORKSPACE_ID"),
        "vk_workspace_branch": os.environ.get("VK_WORKSPACE_BRANCH"),
        "vk_execution_purpose": os.environ.get("VK_EXECUTION_PURPOSE"),
        "vk_repo_names": os.environ.get("VK_REPO_NAMES"),
    }


def get_claude_account_id() -> str | None:
    """
    Get a unique identifier for the current Claude account.

    Reads the access token from ~/.claude/.credentials.json and hashes it
    to create a privacy-preserving unique identifier per account.
    """
    credentials_path = Path.home() / ".claude" / ".credentials.json"
    try:
        with open(credentials_path, "r", encoding="utf-8") as f:
            credentials = json.load(f)
        token = credentials.get("claudeAiOauth", {}).get("accessToken")
        if token:
            return hashlib.sha256(token.encode()).hexdigest()[:16]
    except (OSError, json.JSONDecodeError, KeyError):
        pass
    return None


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


def extract_background_task_id(tool_output) -> str | None:
    """
    Extract background task ID from Bash tool output.

    Parses output for pattern: "Command running in background with ID: <task_id>"

    Args:
        tool_output: Tool result content - can be:
            - Plain string
            - List of content blocks (each may be string or dict with type: text)
            - Dict with type: text and text field

    Returns:
        The task_id string if found, None otherwise.
    """
    if tool_output is None:
        return None

    # Normalize to string for pattern matching
    text_content = ""
    if isinstance(tool_output, str):
        text_content = tool_output
    elif isinstance(tool_output, list):
        # List of content blocks
        parts = []
        for block in tool_output:
            if isinstance(block, str):
                parts.append(block)
            elif isinstance(block, dict) and block.get("type") == "text":
                parts.append(block.get("text", ""))
        text_content = "\n".join(parts)
    elif isinstance(tool_output, dict) and tool_output.get("type") == "text":
        text_content = tool_output.get("text", "")

    # Match the background task pattern
    # Use [a-zA-Z0-9]+ to avoid capturing trailing punctuation (e.g., period)
    match = re.search(r"Command running in background with ID:\s*([a-zA-Z0-9]+)", text_content)
    if match:
        return match.group(1)
    return None


def extract_task_output_info(tool_name: str, tool_input: dict | None) -> tuple[str | None, bool]:
    """
    Extract task ID and blocking status from TaskOutput tool calls.

    Args:
        tool_name: The name of the tool being called
        tool_input: The tool input dict (may contain task_id, block)

    Returns:
        Tuple of (task_id, is_blocking):
            - task_id: The task ID being retrieved, or None if not a TaskOutput call
            - is_blocking: True if block=true (default), False otherwise
    """
    if tool_name != "TaskOutput":
        return (None, False)

    tool_input = tool_input or {}
    task_id = tool_input.get("task_id")
    # block defaults to true in TaskOutput
    is_blocking = tool_input.get("block", True)

    return (task_id, is_blocking)


def create_background_umbrella_span(
    background_task_id: str,
    pending_task: dict,
    completion_time: datetime,
    trace_id: str,
    generation_id: str,
    ingestion_now: str,
):
    """
    Create an umbrella span for a background task that has completed.

    This span represents the wall-clock duration of a background task from when
    it was started until its output was retrieved via TaskOutput.

    Args:
        background_task_id: The ID of the background task (e.g., "be2438c")
        pending_task: Dict with info about when the task was started:
            - bash_tool_use_id: The original Bash tool_use_id that started the task
            - start_time: datetime when the background task was started
            - activity_kind: The classified activity kind (e.g., "BUILD")
            - tool_name: The tool that started the task (e.g., "Bash")
        completion_time: datetime when TaskOutput retrieved the result
        trace_id: The trace ID for this session
        generation_id: The parent generation ID for the umbrella span
        ingestion_now: ISO timestamp string for the ingestion event

    Returns:
        IngestionEvent_SpanCreate for the umbrella span
    """
    # Import here to avoid circular dependency issues at module load
    from langfuse.api.resources.ingestion.types import IngestionEvent_SpanCreate
    from langfuse.api.resources.ingestion.types.create_span_body import CreateSpanBody

    bash_tool_use_id = pending_task.get("bash_tool_use_id", "")
    start_time = pending_task.get("start_time")
    activity_kind = pending_task.get("activity_kind", "OTHER")
    tool_name = pending_task.get("tool_name", "Bash")

    # Generate deterministic span ID for idempotent upserts
    span_id = generate_deterministic_id(f"umbrella_{bash_tool_use_id}_{background_task_id}")

    return IngestionEvent_SpanCreate(
        id=str(uuid.uuid4()),  # Event ID must be unique per request
        timestamp=ingestion_now,
        body=CreateSpanBody(
            id=span_id,
            trace_id=trace_id,
            parent_observation_id=generation_id,
            name=f"BACKGROUND/{activity_kind}/{tool_name}",
            input={"background_task_id": background_task_id},
            output=None,  # Output will be set when we have the actual result
            start_time=start_time,
            end_time=completion_time,
            metadata={
                "activity_kind": activity_kind,
                "background_task_id": background_task_id,
                "bash_tool_use_id": bash_tool_use_id,
                "is_background": True,
            },
        ),
    )


def parse_transcript(transcript_path: str) -> dict:
    """
    Parse the Claude Code transcript JSONL file and extract complete conversation structure.

    Returns a dict with:
        - session_metadata: {cwd, git_branch, model}
        - turns: List of {user_message, assistant_response} pairs
        - totals: {input_tokens, output_tokens, cache_read_input_tokens,
                   cache_creation_input_tokens, activity_counts}
    """
    # Initialize activity counts with all categories
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

    result = {
        "session_metadata": {"cwd": None, "git_branch": None, "model": None},
        "turns": [],
        "tool_results": {},  # Map of tool_use_id -> result content
        "totals": {
            "input_tokens": 0,
            "output_tokens": 0,
            "cache_read_input_tokens": 0,
            "cache_creation_input_tokens": 0,
            "activity_counts": activity_counts,
        },
    }

    path = Path(transcript_path.replace("~", str(Path.home())))
    if not path.exists():
        return result

    pending_user_message: str | None = None
    pending_user_timestamp: str | None = None

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
                pending_user_timestamp = entry.get("timestamp")
                # Handle content being a plain string vs a list of blocks
                if isinstance(content, str):
                    pending_user_message = content if content else None
                else:
                    text_parts = []
                    for block in content:
                        if isinstance(block, str):
                            text_parts.append(block)
                        elif isinstance(block, dict):
                            block_type = block.get("type")
                            if block_type == "text":
                                text_parts.append(block.get("text", ""))
                            elif block_type == "tool_result":
                                # Store tool results for later matching with spans
                                tool_use_id = block.get("tool_use_id")
                                if tool_use_id:
                                    result["tool_results"][tool_use_id] = block.get("content")
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

                # Create turn entry with timestamps
                assistant_timestamp = entry.get("timestamp")
                turn = {
                    "user_message": pending_user_message,
                    "user_timestamp": pending_user_timestamp,
                    "assistant_response": {
                        "model": model,
                        "text_content": "\n".join(text_parts) if text_parts else None,
                        "usage": assistant_usage,
                        "tool_calls": tool_calls,
                        "timestamp": assistant_timestamp,
                    },
                }
                result["turns"].append(turn)
                pending_user_message = None
                pending_user_timestamp = None

    return result


def generate_deterministic_id(seed: str, prefix: str = "") -> str:
    """
    Generate a deterministic UUID-like ID from a seed string.

    This ensures that the same conversation data sent from different sessions
    (e.g., task session and feedback session) produces the same observation IDs,
    allowing Langfuse to upsert rather than create duplicates.

    Args:
        seed: A stable identifier (e.g., timestamp + content hash)
        prefix: Optional prefix for debugging (not included in ID)

    Returns:
        A 32-character hex string suitable for Langfuse observation IDs
    """
    return hashlib.sha256(seed.encode()).hexdigest()[:32]


def parse_iso_timestamp(ts: str | None) -> datetime | None:
    """Parse ISO timestamp string to timezone-aware datetime."""
    if not ts:
        return None
    try:
        # Handle various ISO formats (with/without microseconds, Z suffix)
        ts = ts.replace("Z", "+00:00")
        dt = datetime.fromisoformat(ts)
        # Ensure timezone-aware
        if dt.tzinfo is None:
            dt = dt.replace(tzinfo=timezone.utc)
        return dt
    except (ValueError, TypeError):
        return None


def send_to_langfuse(session_id: str, parsed: dict, vk_context: dict[str, str | None]) -> None:
    """
    Send parsed transcript traces to Langfuse using low-level ingestion API.

    Uses explicit startTime/endTime from transcript timestamps for accurate duration tracking.

    Creates:
        Trace: claude-code-session
        ├── session_id, metadata (vk_*, model, git_branch, activity_counts, totals)
        ├── input: first user message
        ├── output: last assistant text
        │
        ├── Generation: "llm-response-{i}"
        │   ├── model, input (user message), output (assistant text)
        │   ├── startTime: user message timestamp, endTime: assistant response timestamp
        │   ├── usageDetails: {input, output, cache_read, cache_creation}
        │   │
        │   └── Span: "{tool_name}" (child for each tool_call)
        │       ├── input: tool_input dict
        │       └── metadata: {activity_kind, tool_use_id}
    """
    try:
        from langfuse import Langfuse
        from langfuse.api.resources.ingestion.types import (
            IngestionEvent_GenerationCreate,
            IngestionEvent_SpanCreate,
            IngestionEvent_TraceCreate,
        )
        from langfuse.api.resources.ingestion.types.create_generation_body import (
            CreateGenerationBody,
        )
        from langfuse.api.resources.ingestion.types.create_span_body import (
            CreateSpanBody,
        )
        from langfuse.api.resources.ingestion.types.trace_body import TraceBody
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

    turns = parsed.get("turns", [])
    totals = parsed.get("totals", {})
    tool_results = parsed.get("tool_results", {})
    session_metadata = parsed.get("session_metadata", {})

    # Extract first/last timestamps for trace-level timing
    first_timestamp: datetime | None = None
    last_timestamp: datetime | None = None
    first_user_message: str | None = None
    last_assistant_text: str | None = None

    for turn in turns:
        user_ts = parse_iso_timestamp(turn.get("user_timestamp"))
        assistant_ts = parse_iso_timestamp(turn.get("assistant_response", {}).get("timestamp"))

        if user_ts and first_timestamp is None:
            first_timestamp = user_ts
        if assistant_ts:
            last_timestamp = assistant_ts

        user_msg = turn.get("user_message")
        if user_msg and first_user_message is None:
            first_user_message = user_msg
        assistant_text = turn.get("assistant_response", {}).get("text_content")
        if assistant_text:
            last_assistant_text = assistant_text

    # Fallback timestamps if parsing fails
    now = datetime.now(timezone.utc)
    if first_timestamp is None:
        first_timestamp = now
    if last_timestamp is None:
        last_timestamp = now

    # Count total tool calls
    total_tool_calls = sum(
        len(turn.get("assistant_response", {}).get("tool_calls", []))
        for turn in turns
    )

    # Build trace metadata including VK context
    trace_metadata = {
        "source": "claude-code",
        "hook": "langfuse-hook",
        "session_id": session_id,  # Keep original session_id for debugging
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

    # Build tags for filtering
    # Always include "vk" tag when running in vibe-kanban context
    # Add execution purpose as additional tag if set (e.g., "task", "feedback")
    tags: list[str] = []
    if any(v is not None for v in vk_context.values()):
        tags.append("vk")
    execution_purpose = vk_context.get("vk_execution_purpose")
    if execution_purpose:
        tags.append(execution_purpose)

    # Build ingestion events
    events = []
    # Langfuse data model:
    # - Session: groups multiple traces (use vk_task_id to group all runs for same task)
    # - Trace: single complete operation (each Claude Code session is one trace)
    trace_id = session_id
    langfuse_session_id = vk_context.get("vk_task_id")  # None if not in vibe-kanban context
    ingestion_now = now.isoformat()
    account_id = get_claude_account_id()

    # Build tags for filtering in Langfuse UI
    tags = ["claude-code"]
    if langfuse_session_id:
        tags.append("vibe-kanban")

    # Create trace event
    events.append(
        IngestionEvent_TraceCreate(
            id=str(uuid.uuid4()),
            timestamp=ingestion_now,
            body=TraceBody(
                id=trace_id,
                name="claude-code-session",
                session_id=langfuse_session_id,
                user_id=account_id,
                input=first_user_message,
                output=last_assistant_text,
                metadata=trace_metadata,
                tags=tags if tags else None,
                timestamp=first_timestamp,
            ),
        )
    )

    # Create generation and span events for each turn
    # Track previous end_time for continuations without user_timestamp
    prev_end_time: datetime | None = None

    # Track background tasks started but not yet completed (Bash with run_in_background)
    # Maps background_task_id -> {
    #   "bash_tool_use_id": str,
    #   "generation_id": str,
    #   "original_activity": str,
    #   "tool_name": str,
    #   "start_time": datetime,
    #   "command": str | None,
    # }
    pending_background_tasks: dict[str, dict] = {}

    for i, turn in enumerate(turns):
        user_message = turn.get("user_message")
        user_timestamp = parse_iso_timestamp(turn.get("user_timestamp"))
        assistant_response = turn.get("assistant_response", {})
        model = assistant_response.get("model")
        text_content = assistant_response.get("text_content")
        usage = assistant_response.get("usage", {})
        tool_calls = assistant_response.get("tool_calls", [])
        assistant_timestamp = parse_iso_timestamp(assistant_response.get("timestamp"))

        # Determine start_time: prefer user_timestamp, then prev_end_time, then first_timestamp
        # This handles continuations after tool results where there's no user message
        if user_timestamp:
            start_time = user_timestamp
        elif prev_end_time:
            start_time = prev_end_time
        else:
            start_time = first_timestamp

        # Determine end_time: prefer assistant_timestamp, then use start_time (instant)
        end_time = assistant_timestamp or start_time

        # Generate deterministic generation ID from stable data
        # This ensures inherited turns from task session get the same ID in feedback session
        # Using: turn timestamp + user message content (both are identical in inherited transcript)
        gen_seed_parts = [
            turn.get("user_timestamp") or "",
            turn.get("assistant_response", {}).get("timestamp") or "",
            user_message or "",
        ]
        generation_id = generate_deterministic_id("|".join(gen_seed_parts))

        # Create generation event with accurate timestamps
        events.append(
            IngestionEvent_GenerationCreate(
                id=str(uuid.uuid4()),
                timestamp=ingestion_now,
                body=CreateGenerationBody(
                    id=generation_id,
                    trace_id=trace_id,
                    name=f"llm-response-{i}",
                    model=model,
                    input=user_message,
                    output=text_content,
                    start_time=start_time,
                    end_time=end_time,
                    usage_details={
                        "input": usage.get("input_tokens", 0),
                        "output": usage.get("output_tokens", 0),
                        "total": usage.get("input_tokens", 0) + usage.get("output_tokens", 0),
                        # Use Langfuse's expected cache key names for proper cost calculation
                        "input_cache_read": usage.get("cache_read_input_tokens", 0),
                        "input_cache_creation": usage.get("cache_creation_input_tokens", 0),
                    },
                    metadata={"tool_call_count": len(tool_calls)},
                ),
            )
        )

        # Create span events for tool calls as children of the generation
        for tool_call in tool_calls:
            tool_name = tool_call["tool_name"]
            tool_use_id = tool_call.get("tool_use_id")
            tool_input = tool_call.get("tool_input")
            activity_kind = tool_call["activity_kind"]

            # Look up tool result by tool_use_id to add as span output
            tool_output = tool_results.get(tool_use_id) if tool_use_id else None

            # Build mutable metadata dict
            span_metadata: dict = {
                "activity_kind": activity_kind,
                "tool_use_id": tool_use_id,
            }

            # Detect background task starts from Bash output
            if tool_name == "Bash" and tool_output:
                background_task_id = extract_background_task_id(tool_output)
                if background_task_id:
                    pending_background_tasks[background_task_id] = {
                        "bash_tool_use_id": tool_use_id,
                        "generation_id": generation_id,
                        "activity_kind": activity_kind,
                        "tool_name": tool_name,
                        "start_time": start_time,
                        "command": tool_input.get("command") if tool_input else None,
                    }
                    span_metadata["is_background_start"] = True
                    span_metadata["background_task_id"] = background_task_id

            # Detect TaskOutput completions and create umbrella spans
            task_id, is_blocking = extract_task_output_info(tool_name, tool_input)
            if task_id and is_blocking and task_id in pending_background_tasks:
                pending_task = pending_background_tasks[task_id]
                wall_time = (end_time - pending_task["start_time"]).total_seconds()

                # Create umbrella span for the background task
                umbrella_span = create_background_umbrella_span(
                    background_task_id=task_id,
                    pending_task=pending_task,
                    completion_time=end_time,
                    trace_id=trace_id,
                    generation_id=pending_task["generation_id"],
                    ingestion_now=ingestion_now,
                )
                events.append(umbrella_span)

                # Add completion metadata to the TaskOutput span
                span_metadata["is_background_completion"] = True
                span_metadata["background_task_id"] = task_id
                span_metadata["background_wall_time_seconds"] = wall_time

                # Remove from pending dict
                del pending_background_tasks[task_id]

            # Generate deterministic span ID from tool_use_id (assigned by Claude API, stable across sessions)
            span_id = generate_deterministic_id(tool_use_id) if tool_use_id else str(uuid.uuid4())
            events.append(
                IngestionEvent_SpanCreate(
                    id=str(uuid.uuid4()),  # Event ID must be unique per request
                    timestamp=ingestion_now,
                    body=CreateSpanBody(
                        id=span_id,  # Body ID is deterministic for upsert
                        trace_id=trace_id,
                        parent_observation_id=generation_id,
                        name=f"{activity_kind}/{tool_name}",
                        input=tool_input,
                        output=tool_output,
                        start_time=start_time,
                        end_time=end_time,
                        metadata=span_metadata,
                    ),
                )
            )

        # Update prev_end_time for next iteration
        prev_end_time = end_time

    # Warn about background tasks that were started but never completed
    if pending_background_tasks:
        debug_log(f"Warning: {len(pending_background_tasks)} background tasks not completed: {list(pending_background_tasks.keys())}")

    # Send batch to Langfuse
    try:
        langfuse.api.ingestion.batch(batch=events)
        debug_log(f"Sent {len(events)} events to Langfuse via batch API")
    except Exception as e:
        debug_log(f"Error sending batch to Langfuse: {e}")
        raise


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

    # Parse transcript
    try:
        parsed = parse_transcript(transcript_path)
        debug_log(f"Parsed {len(parsed.get('turns', []))} turns")
    except Exception as e:
        debug_log(f"Error parsing transcript: {e}")
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
        print(f"Parsed transcript: {json.dumps(parsed, indent=2)}", file=sys.stderr)

    print(
        f"Langfuse hook: {turn_count} turns, {total_tool_calls} tool calls, "
        f"{totals.get('input_tokens', 0)} input tokens, {totals.get('output_tokens', 0)} output tokens",
        file=sys.stderr,
    )

    # Send to Langfuse
    try:
        debug_log("Sending to Langfuse...")
        send_to_langfuse(session_id, parsed, vk_context)
        debug_log("Successfully sent to Langfuse")
    except Exception as e:
        debug_log(f"Error sending to Langfuse: {e}")
        print(f"Error sending to Langfuse: {e}", file=sys.stderr)
        return 0

    return 0


if __name__ == "__main__":
    sys.exit(main())
