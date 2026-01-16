-- Add conversation_session_id to execution_processes for disposable conversations
-- Also update run_reason CHECK to include 'disposableconversation' and 'internalagent'
-- And make session_id nullable for conversation-based executions

PRAGMA foreign_keys = OFF;

-- Rebuild execution_processes with nullable session_id and conversation_session_id
CREATE TABLE execution_processes_new (
    id                       BLOB PRIMARY KEY,
    session_id               BLOB,  -- Now nullable for conversation-based executions
    conversation_session_id  BLOB,  -- New: link to conversation_sessions for disposable conversations
    run_reason               TEXT NOT NULL DEFAULT 'setupscript'
                               CHECK (run_reason IN ('setupscript','codingagent','devserver','cleanupscript','internalagent','disposableconversation')),
    executor_action          TEXT NOT NULL DEFAULT '{}',
    status                   TEXT NOT NULL DEFAULT 'running'
                               CHECK (status IN ('running','completed','failed','killed')),
    exit_code                INTEGER,
    dropped                  INTEGER NOT NULL DEFAULT 0,
    input_tokens             INTEGER,
    output_tokens            INTEGER,
    started_at               TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    completed_at             TEXT,
    created_at               TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at               TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (conversation_session_id) REFERENCES conversation_sessions(id) ON DELETE CASCADE,
    -- Ensure at least one session reference exists
    CHECK (session_id IS NOT NULL OR conversation_session_id IS NOT NULL)
);

-- Copy existing data
INSERT INTO execution_processes_new (
    id, session_id, conversation_session_id, run_reason, executor_action, status,
    exit_code, dropped, input_tokens, output_tokens, started_at, completed_at, created_at, updated_at
)
SELECT
    id, session_id, NULL, run_reason, executor_action, status,
    exit_code, dropped, input_tokens, output_tokens, started_at, completed_at, created_at, updated_at
FROM execution_processes;

-- Drop old table and rename
DROP TABLE execution_processes;
ALTER TABLE execution_processes_new RENAME TO execution_processes;

-- Recreate indexes
CREATE INDEX idx_execution_processes_session_id ON execution_processes(session_id);
CREATE INDEX idx_execution_processes_conversation_session_id ON execution_processes(conversation_session_id);
CREATE INDEX idx_execution_processes_status ON execution_processes(status);
CREATE INDEX idx_execution_processes_run_reason ON execution_processes(run_reason);

-- Composite indexes
CREATE INDEX idx_execution_processes_session_status_run_reason
ON execution_processes (session_id, status, run_reason);

CREATE INDEX idx_execution_processes_session_run_reason_created
ON execution_processes (session_id, run_reason, created_at DESC);

CREATE INDEX idx_execution_processes_conversation_run_reason_created
ON execution_processes (conversation_session_id, run_reason, created_at DESC);

PRAGMA foreign_key_check;
PRAGMA foreign_keys = ON;
