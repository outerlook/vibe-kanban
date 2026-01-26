-- Add needs_attention column to tasks table and create review_attention table
-- for storing review attention analysis results from agents

-- Add needs_attention column to tasks table
-- NULL = not yet analyzed, true = needs attention, false = no attention needed
ALTER TABLE tasks ADD COLUMN needs_attention INTEGER;

-- Create review_attention table for storing detailed analysis results
CREATE TABLE review_attention (
    id                      BLOB PRIMARY KEY,
    execution_process_id    BLOB NOT NULL UNIQUE,
    task_id                 BLOB NOT NULL,
    workspace_id            BLOB NOT NULL,
    needs_attention         INTEGER NOT NULL,
    reasoning               TEXT,
    analyzed_at             TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    created_at              TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at              TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (execution_process_id) REFERENCES execution_processes(id) ON DELETE CASCADE,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);

-- Create indexes for efficient lookups
CREATE UNIQUE INDEX idx_review_attention_execution_process_id ON review_attention(execution_process_id);
CREATE INDEX idx_review_attention_task_id ON review_attention(task_id);
CREATE INDEX idx_review_attention_workspace_id ON review_attention(workspace_id);
CREATE INDEX idx_review_attention_analyzed_at ON review_attention(analyzed_at DESC);
