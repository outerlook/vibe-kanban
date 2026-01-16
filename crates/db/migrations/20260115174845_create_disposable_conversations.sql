PRAGMA foreign_keys = ON;

-- Create conversation_sessions table for project-scoped disposable conversations
-- These are lightweight chat sessions NOT tied to workspaces or git worktrees

CREATE TABLE conversation_sessions (
    id          BLOB PRIMARY KEY,
    project_id  BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title       TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'archived')),
    executor    TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec'))
);

CREATE INDEX idx_conversation_sessions_project_id ON conversation_sessions(project_id);
CREATE INDEX idx_conversation_sessions_status ON conversation_sessions(status);

-- Create conversation_messages table for messages within a conversation session

CREATE TABLE conversation_messages (
    id                       BLOB PRIMARY KEY,
    conversation_session_id  BLOB NOT NULL REFERENCES conversation_sessions(id) ON DELETE CASCADE,
    execution_process_id     BLOB REFERENCES execution_processes(id) ON DELETE SET NULL,
    role                     TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    content                  TEXT NOT NULL,
    metadata                 TEXT,
    created_at               TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at               TEXT NOT NULL DEFAULT (datetime('now', 'subsec'))
);

CREATE INDEX idx_conversation_messages_conversation_session_id ON conversation_messages(conversation_session_id);
CREATE INDEX idx_conversation_messages_execution_process_id ON conversation_messages(execution_process_id);
