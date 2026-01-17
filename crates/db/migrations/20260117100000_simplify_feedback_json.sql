-- Simplify agent_feedback table to use a single JSON column instead of 5 separate columns

-- Add the new feedback_json column
ALTER TABLE agent_feedback ADD COLUMN feedback_json TEXT;

-- Migrate existing data to JSON format
UPDATE agent_feedback
SET feedback_json = json_object(
    'task_clarity', task_clarity,
    'missing_tools', missing_tools,
    'integration_problems', integration_problems,
    'improvement_suggestions', improvement_suggestions,
    'agent_documentation', agent_documentation
)
WHERE task_clarity IS NOT NULL
   OR missing_tools IS NOT NULL
   OR integration_problems IS NOT NULL
   OR improvement_suggestions IS NOT NULL
   OR agent_documentation IS NOT NULL;

-- SQLite doesn't support DROP COLUMN in older versions, so we recreate the table
-- Create new table without the old columns
CREATE TABLE agent_feedback_new (
    id                      BLOB PRIMARY KEY,
    execution_process_id    BLOB NOT NULL,
    task_id                 BLOB NOT NULL,
    workspace_id            BLOB NOT NULL,
    feedback_json           TEXT,
    collected_at            TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    created_at              TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at              TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (execution_process_id) REFERENCES execution_processes(id) ON DELETE CASCADE,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);

-- Copy data to new table
INSERT INTO agent_feedback_new (id, execution_process_id, task_id, workspace_id, feedback_json, collected_at, created_at, updated_at)
SELECT id, execution_process_id, task_id, workspace_id, feedback_json, collected_at, created_at, updated_at
FROM agent_feedback;

-- Drop old table and rename new one
DROP TABLE agent_feedback;
ALTER TABLE agent_feedback_new RENAME TO agent_feedback;

-- Recreate indexes
CREATE INDEX idx_agent_feedback_execution_process_id ON agent_feedback(execution_process_id);
CREATE INDEX idx_agent_feedback_task_id ON agent_feedback(task_id);
CREATE INDEX idx_agent_feedback_collected_at ON agent_feedback(collected_at DESC);
