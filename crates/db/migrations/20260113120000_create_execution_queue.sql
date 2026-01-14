PRAGMA foreign_keys = ON;

-- Create execution_queue table for global concurrency limiter
-- Entries exist while waiting, deleted when started
CREATE TABLE execution_queue (
    id                  BLOB PRIMARY KEY,
    workspace_id        BLOB NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    executor_profile_id TEXT NOT NULL,
    queued_at           TEXT NOT NULL DEFAULT (datetime('now', 'subsec'))
);

CREATE INDEX idx_execution_queue_queued_at ON execution_queue(queued_at);
