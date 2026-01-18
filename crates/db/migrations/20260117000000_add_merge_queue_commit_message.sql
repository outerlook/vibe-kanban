-- Add commit_message column to support user/LLM-provided commit messages
ALTER TABLE merge_queue ADD COLUMN commit_message TEXT;
