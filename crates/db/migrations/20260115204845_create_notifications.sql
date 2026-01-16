PRAGMA foreign_keys = ON;

-- Create notifications table for in-app notification system
CREATE TABLE notifications (
    id                BLOB PRIMARY KEY,
    project_id        BLOB REFERENCES projects(id) ON DELETE CASCADE,
    notification_type TEXT NOT NULL CHECK (notification_type IN ('agent_complete', 'agent_approval_needed', 'agent_error', 'conversation_response')),
    title             TEXT NOT NULL,
    message           TEXT NOT NULL,
    is_read           INTEGER NOT NULL DEFAULT 0,
    metadata          TEXT,
    workspace_id      BLOB REFERENCES workspaces(id) ON DELETE CASCADE,
    session_id        BLOB REFERENCES sessions(id) ON DELETE CASCADE,
    created_at        TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now', 'subsec'))
);

-- Indexes for common query patterns
CREATE INDEX idx_notifications_project_id ON notifications(project_id);
CREATE INDEX idx_notifications_workspace_id ON notifications(workspace_id);
CREATE INDEX idx_notifications_session_id ON notifications(session_id);
CREATE INDEX idx_notifications_is_read ON notifications(is_read);
CREATE INDEX idx_notifications_created_at_desc ON notifications(created_at DESC);
