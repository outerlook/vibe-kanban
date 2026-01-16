PRAGMA foreign_keys = ON;

-- Create merge_queue table for FIFO merge operations
CREATE TABLE merge_queue (
    id               BLOB PRIMARY KEY,
    project_id       BLOB NOT NULL,
    workspace_id     BLOB NOT NULL,
    repo_id          BLOB NOT NULL,
    queued_at        TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    status           TEXT NOT NULL DEFAULT 'queued'
                        CHECK (status IN ('queued', 'merging', 'conflict', 'completed')),
    conflict_message TEXT,
    started_at       TEXT,
    completed_at     TEXT,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    FOREIGN KEY (repo_id) REFERENCES repos(id) ON DELETE CASCADE
);

-- Index for efficient FIFO pop: get oldest queued item for a project
CREATE INDEX idx_merge_queue_project_queued ON merge_queue(project_id, queued_at);
