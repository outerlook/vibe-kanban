-- Add materialized status columns to tasks table to eliminate expensive subqueries
-- These columns cache computed status values that were previously calculated per-query

-- Add the new columns with defaults
ALTER TABLE tasks ADD COLUMN is_blocked INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN has_in_progress_attempt INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN last_attempt_failed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN is_queued INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN last_executor TEXT NOT NULL DEFAULT '';

-- Populate is_blocked: 1 if any dependency is not done
UPDATE tasks SET is_blocked = (
  SELECT CASE WHEN COUNT(*) > 0 THEN 1 ELSE 0 END
  FROM task_dependencies td
  JOIN tasks dep ON dep.id = td.depends_on_id
  WHERE td.task_id = tasks.id AND dep.status != 'done'
);

-- Populate has_in_progress_attempt: 1 if any execution process is running
UPDATE tasks SET has_in_progress_attempt = (
  SELECT CASE WHEN COUNT(*) > 0 THEN 1 ELSE 0 END
  FROM workspaces w
  JOIN sessions s ON s.workspace_id = w.id
  JOIN execution_processes ep ON ep.session_id = s.id
  WHERE w.task_id = tasks.id
    AND ep.status = 'running'
    AND ep.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
);

-- Populate last_attempt_failed: 1 if the most recent attempt failed or was killed
UPDATE tasks SET last_attempt_failed = COALESCE((
  SELECT CASE WHEN ep_status IN ('failed', 'killed') THEN 1 ELSE 0 END
  FROM (
    SELECT ep.status AS ep_status
    FROM workspaces w
    JOIN sessions s ON s.workspace_id = w.id
    JOIN execution_processes ep ON ep.session_id = s.id
    WHERE w.task_id = tasks.id
      AND ep.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
    ORDER BY ep.created_at DESC
    LIMIT 1
  )
), 0);

-- Populate is_queued: 1 if task has an entry in execution_queue
UPDATE tasks SET is_queued = (
  SELECT CASE WHEN COUNT(*) > 0 THEN 1 ELSE 0 END
  FROM workspaces w
  JOIN execution_queue eq ON eq.workspace_id = w.id
  WHERE w.task_id = tasks.id
);

-- Populate last_executor: executor from most recent session
UPDATE tasks SET last_executor = COALESCE((
  SELECT s.executor
  FROM workspaces w
  JOIN sessions s ON s.workspace_id = w.id
  WHERE w.task_id = tasks.id
  ORDER BY s.created_at DESC
  LIMIT 1
), '');
