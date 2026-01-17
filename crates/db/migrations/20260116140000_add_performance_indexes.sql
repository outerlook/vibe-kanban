PRAGMA foreign_keys = ON;

-- Add missing indexes for FK lookups to improve task listing query performance

-- Index on workspaces(task_id) for joining tasks to workspaces
CREATE INDEX IF NOT EXISTS idx_workspaces_task_id
ON workspaces(task_id);

-- Index on execution_queue(workspace_id) for workspace lookups
CREATE INDEX IF NOT EXISTS idx_execution_queue_workspace_id
ON execution_queue(workspace_id);

-- Composite index on conversation_messages for ordered message retrieval
-- Supports queries like: SELECT * FROM conversation_messages WHERE conversation_session_id = ? ORDER BY created_at DESC, id DESC
CREATE INDEX IF NOT EXISTS idx_conversation_messages_session_created_desc
ON conversation_messages(conversation_session_id, created_at DESC, id DESC);
