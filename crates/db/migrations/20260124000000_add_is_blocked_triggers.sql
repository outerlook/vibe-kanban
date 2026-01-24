-- =============================================================================
-- Triggers to maintain materialized status columns on tasks table
-- These replace manual update_materialized_status calls with automatic updates
-- =============================================================================

-- =============================================================================
-- is_blocked triggers (task_dependencies and task status changes)
-- =============================================================================

-- Trigger to update is_blocked when a dependency is deleted (including CASCADE deletes)
CREATE TRIGGER update_is_blocked_after_dependency_delete
AFTER DELETE ON task_dependencies
FOR EACH ROW
BEGIN
    UPDATE tasks
    SET is_blocked = (
        SELECT CASE WHEN COUNT(*) > 0 THEN 1 ELSE 0 END
        FROM task_dependencies td
        JOIN tasks dep ON dep.id = td.depends_on_id
        WHERE td.task_id = OLD.task_id AND dep.status != 'done'
    )
    WHERE id = OLD.task_id;
END;

-- Trigger to update is_blocked when a dependency is added
CREATE TRIGGER update_is_blocked_after_dependency_insert
AFTER INSERT ON task_dependencies
FOR EACH ROW
BEGIN
    UPDATE tasks
    SET is_blocked = (
        SELECT CASE WHEN COUNT(*) > 0 THEN 1 ELSE 0 END
        FROM task_dependencies td
        JOIN tasks dep ON dep.id = td.depends_on_id
        WHERE td.task_id = NEW.task_id AND dep.status != 'done'
    )
    WHERE id = NEW.task_id;
END;

-- Trigger to update is_blocked for dependent tasks when a task's status changes
CREATE TRIGGER update_dependents_is_blocked_after_task_status_change
AFTER UPDATE OF status ON tasks
FOR EACH ROW
WHEN OLD.status != NEW.status
BEGIN
    UPDATE tasks
    SET is_blocked = (
        SELECT CASE WHEN COUNT(*) > 0 THEN 1 ELSE 0 END
        FROM task_dependencies td
        JOIN tasks dep ON dep.id = td.depends_on_id
        WHERE td.task_id = tasks.id AND dep.status != 'done'
    )
    WHERE id IN (
        SELECT task_id FROM task_dependencies WHERE depends_on_id = NEW.id
    );
END;

-- =============================================================================
-- is_queued triggers (execution_queue changes)
-- =============================================================================

-- When an entry is added to execution_queue, mark the task as queued
CREATE TRIGGER update_is_queued_after_queue_insert
AFTER INSERT ON execution_queue
FOR EACH ROW
BEGIN
    UPDATE tasks
    SET is_queued = 1
    WHERE id = (
        SELECT w.task_id FROM workspaces w WHERE w.id = NEW.workspace_id
    );
END;

-- When an entry is removed from execution_queue, recalculate is_queued
CREATE TRIGGER update_is_queued_after_queue_delete
AFTER DELETE ON execution_queue
FOR EACH ROW
BEGIN
    UPDATE tasks
    SET is_queued = (
        SELECT CASE WHEN COUNT(*) > 0 THEN 1 ELSE 0 END
        FROM workspaces w
        JOIN execution_queue eq ON eq.workspace_id = w.id
        WHERE w.task_id = tasks.id
    )
    WHERE id = (
        SELECT w.task_id FROM workspaces w WHERE w.id = OLD.workspace_id
    );
END;

-- =============================================================================
-- last_executor trigger (session creation)
-- =============================================================================

-- When a new session is created, update last_executor for the task
CREATE TRIGGER update_last_executor_after_session_insert
AFTER INSERT ON sessions
FOR EACH ROW
BEGIN
    UPDATE tasks
    SET last_executor = NEW.executor
    WHERE id = (
        SELECT w.task_id FROM workspaces w WHERE w.id = NEW.workspace_id
    );
END;

-- =============================================================================
-- has_in_progress_attempt and last_attempt_failed triggers (execution_processes)
-- =============================================================================

-- When an execution_process is inserted, update task status
CREATE TRIGGER update_task_execution_status_after_process_insert
AFTER INSERT ON execution_processes
FOR EACH ROW
WHEN NEW.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
BEGIN
    UPDATE tasks
    SET has_in_progress_attempt = CASE WHEN NEW.status = 'running' THEN 1 ELSE has_in_progress_attempt END,
        last_attempt_failed = CASE WHEN NEW.status IN ('failed', 'killed') THEN 1 ELSE 0 END
    WHERE id = (
        SELECT w.task_id
        FROM sessions s
        JOIN workspaces w ON w.id = s.workspace_id
        WHERE s.id = NEW.session_id
    );
END;

-- When an execution_process status changes, update task status
CREATE TRIGGER update_task_execution_status_after_process_update
AFTER UPDATE OF status ON execution_processes
FOR EACH ROW
WHEN OLD.status != NEW.status AND NEW.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
BEGIN
    UPDATE tasks
    SET has_in_progress_attempt = (
            SELECT CASE WHEN COUNT(*) > 0 THEN 1 ELSE 0 END
            FROM workspaces w
            JOIN sessions s ON s.workspace_id = w.id
            JOIN execution_processes ep ON ep.session_id = s.id
            WHERE w.task_id = tasks.id
              AND ep.status = 'running'
              AND ep.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
        ),
        last_attempt_failed = COALESCE((
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
        ), 0)
    WHERE id = (
        SELECT w.task_id
        FROM sessions s
        JOIN workspaces w ON w.id = s.workspace_id
        WHERE s.id = NEW.session_id
    );
END;

-- When an execution_process is deleted, recalculate task status
CREATE TRIGGER update_task_execution_status_after_process_delete
AFTER DELETE ON execution_processes
FOR EACH ROW
WHEN OLD.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
BEGIN
    UPDATE tasks
    SET has_in_progress_attempt = (
            SELECT CASE WHEN COUNT(*) > 0 THEN 1 ELSE 0 END
            FROM workspaces w
            JOIN sessions s ON s.workspace_id = w.id
            JOIN execution_processes ep ON ep.session_id = s.id
            WHERE w.task_id = tasks.id
              AND ep.status = 'running'
              AND ep.run_reason IN ('setupscript', 'cleanupscript', 'codingagent')
        ),
        last_attempt_failed = COALESCE((
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
        ), 0)
    WHERE id = (
        SELECT w.task_id
        FROM sessions s
        JOIN workspaces w ON w.id = s.workspace_id
        WHERE s.id = OLD.session_id
    );
END;
