-- Add token usage tracking columns to execution_processes
ALTER TABLE execution_processes ADD COLUMN input_tokens INTEGER;
ALTER TABLE execution_processes ADD COLUMN output_tokens INTEGER;
