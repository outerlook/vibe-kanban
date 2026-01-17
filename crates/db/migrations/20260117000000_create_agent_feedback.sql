-- Create agent_feedback table for storing structured feedback from agents after task completion

CREATE TABLE agent_feedback (
    id                      BLOB PRIMARY KEY,
    execution_process_id    BLOB NOT NULL,
    task_id                 BLOB NOT NULL,
    workspace_id            BLOB NOT NULL,
    task_clarity            TEXT,
    missing_tools           TEXT,
    integration_problems    TEXT,
    improvement_suggestions TEXT,
    agent_documentation     TEXT,
    collected_at            TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    created_at              TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at              TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (execution_process_id) REFERENCES execution_processes(id) ON DELETE CASCADE,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);

CREATE INDEX idx_agent_feedback_execution_process_id ON agent_feedback(execution_process_id);
CREATE INDEX idx_agent_feedback_task_id ON agent_feedback(task_id);
CREATE INDEX idx_agent_feedback_collected_at ON agent_feedback(collected_at DESC);
