-- Add worktree_path to conversation_sessions
-- Stores the path of the worktree where the conversation was started
-- NULL means conversation was started from the main repository

ALTER TABLE conversation_sessions ADD COLUMN worktree_path TEXT;

CREATE INDEX idx_conversation_sessions_worktree_path ON conversation_sessions(worktree_path);
