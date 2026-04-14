CREATE TABLE IF NOT EXISTS workflow_associations (
    id              BLOB PRIMARY KEY,
    project_id      BLOB REFERENCES projects(id) ON DELETE CASCADE,
    task_group_id   BLOB REFERENCES task_groups(id) ON DELETE CASCADE,
    task_id         BLOB REFERENCES tasks(id) ON DELETE CASCADE,
    scope           TEXT NOT NULL CHECK (scope IN ('repository_default', 'task_group_default', 'task_override')),
    workflow_id     TEXT NOT NULL,
    label           TEXT NOT NULL,
    url             TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    CHECK (
        (project_id IS NOT NULL AND task_group_id IS NULL AND task_id IS NULL AND scope = 'repository_default') OR
        (project_id IS NULL AND task_group_id IS NOT NULL AND task_id IS NULL AND scope = 'task_group_default') OR
        (project_id IS NULL AND task_group_id IS NULL AND task_id IS NOT NULL AND scope = 'task_override')
    ),
    UNIQUE(project_id),
    UNIQUE(task_group_id),
    UNIQUE(task_id)
);

CREATE INDEX IF NOT EXISTS idx_workflow_associations_project_id ON workflow_associations(project_id);
CREATE INDEX IF NOT EXISTS idx_workflow_associations_task_group_id ON workflow_associations(task_group_id);
CREATE INDEX IF NOT EXISTS idx_workflow_associations_task_id ON workflow_associations(task_id);
