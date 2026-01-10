PRAGMA foreign_keys = ON;

-- Create task_groups table
CREATE TABLE task_groups (
    id          BLOB PRIMARY KEY,
    project_id  BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    base_branch TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec'))
);

CREATE INDEX idx_task_groups_project_id ON task_groups(project_id);

-- Add task_group_id column to tasks table
ALTER TABLE tasks ADD COLUMN task_group_id BLOB REFERENCES task_groups(id) ON DELETE SET NULL;

CREATE INDEX idx_tasks_task_group_id ON tasks(task_group_id);
