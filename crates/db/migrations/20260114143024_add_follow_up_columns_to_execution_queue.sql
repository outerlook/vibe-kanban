PRAGMA foreign_keys = ON;

-- Add optional columns for follow-up queue entries
-- When session_id and executor_action are NULL, it's an initial workspace start
-- When they're populated, it's a follow-up execution
ALTER TABLE execution_queue ADD COLUMN session_id BLOB REFERENCES sessions(id) ON DELETE CASCADE;
ALTER TABLE execution_queue ADD COLUMN executor_action TEXT;
