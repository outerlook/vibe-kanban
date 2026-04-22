ALTER TABLE task_groups
ADD COLUMN git_mode TEXT NOT NULL DEFAULT 'managed'
CHECK (git_mode IN ('managed', 'preserve_history'));

ALTER TABLE workspaces
ADD COLUMN git_mode TEXT NOT NULL DEFAULT 'managed'
CHECK (git_mode IN ('managed', 'preserve_history'));
