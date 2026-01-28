# User Question Persistence Test

This document describes the integration test for verifying that pending user questions persist across server restarts and that the follow-up mechanism correctly resumes Claude's execution with context.

## Overview

When Claude calls the `AskUserQuestion` tool, the question is persisted to the database (`user_questions` table). This allows:

1. Questions to survive server restarts
2. Users to answer questions at their own pace (no timeout)
3. Follow-up execution to resume Claude's work even if the original executor died

## Test Scenarios

### Scenario 1: Basic Question Persistence

**Purpose:** Verify that a pending question appears in the UI after server restart.

**Preconditions:**
- Server is running (`pnpm run dev`)
- A project with at least one task exists
- A workspace session is active

**Steps:**

1. **Start an execution that triggers AskUserQuestion**
   - Start working on a task with Claude
   - Claude asks a question via `AskUserQuestion` tool
   - Observe the question appears in the conversation UI with input options

2. **Verify question is in database**
   ```sql
   SELECT * FROM user_questions WHERE status = 'pending';
   ```
   - Should show one row with the question data

3. **Restart the server**
   - Stop the backend (`Ctrl+C` on the terminal running `pnpm run backend:dev:watch`)
   - Restart it (`pnpm run backend:dev:watch`)

4. **Verify question still appears in UI**
   - Navigate to the same task/workspace
   - The pending question should still be visible in the conversation
   - Question options should be interactive

**Expected Result:**
- Question persists in database across restart
- UI loads question from conversation history (message store)
- User can still answer the question

### Scenario 2: Answer After Server Restart (Follow-up Trigger)

**Purpose:** Verify that answering a question after server restart triggers a follow-up execution with the user's response.

**Preconditions:**
- Complete Scenario 1 (question visible after restart)

**Steps:**

1. **Answer the question in the UI**
   - Select option(s) or enter custom text
   - Click "Submit" (or use `Cmd+Shift+Y` / `Ctrl+Shift+Y`)

2. **Observe follow-up execution**
   - A new execution should start automatically
   - Check logs for: `"Starting follow-up execution for answered question {approval_id}"`

3. **Verify Claude receives context**
   - The follow-up prompt includes Q&A summary:
     ```
     The user has answered your question(s). Here is the Q&A:

     **Question:** [original question]
     **Answer:** [user's answer]

     Please continue based on the user's response.
     ```
   - Claude continues working with the answer

4. **Verify database state**
   ```sql
   SELECT * FROM user_questions WHERE approval_id = '<approval_id>';
   ```
   - `status` should be `'answered'`
   - `answers` should contain the JSON response
   - `answered_at` should be populated

**Expected Result:**
- Answer is saved to database
- Follow-up execution is triggered automatically
- Claude receives properly formatted Q&A context
- Task execution continues seamlessly

### Scenario 3: Concurrent Question Handling

**Purpose:** Verify that multiple pending questions from different tasks don't interfere.

**Steps:**

1. Create pending questions in two different task workspaces
2. Restart the server
3. Verify both questions appear in their respective task UIs
4. Answer one question
5. Verify only that task's follow-up is triggered

**Expected Result:**
- Questions are isolated by `execution_process_id`
- Answering one doesn't affect the other

## Key Implementation Details

### Database Schema

```sql
CREATE TABLE user_questions (
    id                      BLOB PRIMARY KEY,
    approval_id             TEXT UNIQUE NOT NULL,
    execution_process_id    BLOB NOT NULL,
    questions               TEXT NOT NULL,  -- JSON array of QuestionData
    answers                 TEXT,           -- JSON array of QuestionAnswer
    status                  TEXT NOT NULL DEFAULT 'pending',
    created_at              TEXT NOT NULL,
    answered_at             TEXT,
    FOREIGN KEY (execution_process_id) REFERENCES execution_processes(id) ON DELETE CASCADE
);
```

### Data Flow

```
┌─────────────┐     ┌──────────────┐     ┌────────────────┐
│ Claude calls│────▶│ ApprovalSvc  │────▶│ user_questions │
│ AskQuestion │     │ persists to  │     │ table (DB)     │
└─────────────┘     │ DB + memory  │     └────────────────┘
                    └──────────────┘
                           │
                           ▼
                    ┌──────────────┐
                    │ MsgStore     │
                    │ (UI update)  │
                    └──────────────┘

[Server restart - memory cleared, DB persists]

┌─────────────┐     ┌──────────────┐     ┌────────────────┐
│ User answers│────▶│ ApprovalSvc  │────▶│ Check DB for   │
│ question    │     │ respond()    │     │ pending Q      │
└─────────────┘     └──────────────┘     └────────────────┘
                           │
                           │ (executor dead, needs_follow_up=true)
                           ▼
                    ┌────────────────────┐
                    │ trigger_follow_up  │
                    │ - Load Q&A from DB │
                    │ - Format prompt    │
                    │ - Start execution  │
                    └────────────────────┘
                           │
                           ▼
                    ┌──────────────┐
                    │ New executor │
                    │ with Q&A     │
                    │ context      │
                    └──────────────┘
```

### API Endpoints

| Endpoint | Purpose |
|----------|---------|
| `POST /api/approvals/{id}/respond` | Submit answer to a question |

### Response Format

```typescript
interface ApprovalResponse {
  execution_process_id: string; // UUID
  status: ApprovalStatus;
  answers?: QuestionAnswer[];
}

interface QuestionAnswer {
  question_index: number;
  selected_indices: number[];
  other_text?: string;
}
```

## Troubleshooting

### Question doesn't appear after restart

1. Check if question is in database:
   ```sql
   SELECT * FROM user_questions WHERE status = 'pending';
   ```

2. Verify the execution_process still exists:
   ```sql
   SELECT * FROM execution_processes WHERE id = '<execution_process_id>';
   ```

3. Check server logs for errors loading conversation history

### Follow-up doesn't trigger

1. Check server logs for:
   - `"Failed to trigger follow-up for answered question"`
   - `"At concurrency limit, queueing follow-up"`

2. Verify execution context is loadable:
   ```sql
   SELECT ep.*, s.*, w.*, t.*, p.*
   FROM execution_processes ep
   JOIN sessions s ON ep.session_id = s.id
   JOIN workspaces w ON s.workspace_id = w.id
   JOIN tasks t ON w.task_id = t.id
   JOIN projects p ON t.project_id = p.id
   WHERE ep.id = '<execution_process_id>';
   ```

3. Check if executor profile exists for the session

### Claude doesn't receive answer context

1. Verify the follow-up prompt format in logs
2. Check that `latest_agent_session_id` was found (determines follow-up vs initial request)

## Related Code

| File | Purpose |
|------|---------|
| `crates/db/src/models/user_question.rs` | Database CRUD operations |
| `crates/services/src/services/approvals.rs` | Question creation & response handling |
| `crates/server/src/routes/approvals.rs` | HTTP endpoint & follow-up trigger |
| `crates/utils/src/approvals.rs` | Q&A formatting utilities |
| `frontend/src/components/NormalizedConversation/PendingUserQuestionEntry.tsx` | UI component |

## Automated Testing

For automated integration testing, see `crates/services/tests/` for examples of setting up test databases and simulating the flow programmatically.

Key test helpers:
- `create_test_db()` - Creates isolated SQLite database with migrations
- `create_test_execution_process()` - Sets up execution context
- `UserQuestion::create()` / `UserQuestion::update_answer()` - Direct DB operations
