PRAGMA foreign_keys = ON;

-- Create app_settings table for global application settings
-- This is a singleton table (single row) for app-wide configuration
CREATE TABLE app_settings (
    id                      INTEGER PRIMARY KEY CHECK (id = 1),
    github_token_encrypted  TEXT,
    created_at              TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at              TEXT NOT NULL DEFAULT (datetime('now', 'subsec'))
);

-- Insert the singleton row
INSERT INTO app_settings (id) VALUES (1);
