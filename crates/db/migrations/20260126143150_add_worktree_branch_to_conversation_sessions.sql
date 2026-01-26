-- Add worktree_branch to conversation_sessions
-- Stores the branch name associated with the worktree
-- This is derived from the worktree path at creation time

ALTER TABLE conversation_sessions ADD COLUMN worktree_branch TEXT;
