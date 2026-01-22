-- Add merge_target_branch to project_repos for configurable merge detection
ALTER TABLE project_repos ADD COLUMN merge_target_branch TEXT DEFAULT 'main';
