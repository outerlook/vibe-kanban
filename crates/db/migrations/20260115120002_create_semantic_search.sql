PRAGMA foreign_keys = ON;

--------------------------------------------------------------------------------
-- FTS5: Full-text search for tasks
--------------------------------------------------------------------------------

-- FTS5 virtual table for full-text search on tasks
-- Uses external content mode to avoid duplicating data
CREATE VIRTUAL TABLE tasks_fts USING fts5(
    title,
    description,
    content='tasks',
    content_rowid='rowid',
    tokenize='porter unicode61'
);

-- Populate FTS5 with existing tasks
INSERT INTO tasks_fts(rowid, title, description)
SELECT rowid, title, COALESCE(description, '') FROM tasks;

-- Trigger to keep FTS5 in sync on INSERT
CREATE TRIGGER tasks_fts_insert AFTER INSERT ON tasks BEGIN
    INSERT INTO tasks_fts(rowid, title, description)
    VALUES (NEW.rowid, NEW.title, COALESCE(NEW.description, ''));
END;

-- Trigger to keep FTS5 in sync on DELETE
CREATE TRIGGER tasks_fts_delete AFTER DELETE ON tasks BEGIN
    INSERT INTO tasks_fts(tasks_fts, rowid, title, description)
    VALUES ('delete', OLD.rowid, OLD.title, COALESCE(OLD.description, ''));
END;

-- Trigger to keep FTS5 in sync on UPDATE
CREATE TRIGGER tasks_fts_update AFTER UPDATE ON tasks BEGIN
    INSERT INTO tasks_fts(tasks_fts, rowid, title, description)
    VALUES ('delete', OLD.rowid, OLD.title, COALESCE(OLD.description, ''));
    INSERT INTO tasks_fts(rowid, title, description)
    VALUES (NEW.rowid, NEW.title, COALESCE(NEW.description, ''));
END;

--------------------------------------------------------------------------------
-- Embedding status tracking (for future sqlite-vec integration)
--------------------------------------------------------------------------------

-- Tracking table for embedding status
CREATE TABLE task_embedding_status (
    task_id         BLOB PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    needs_embedding INTEGER NOT NULL DEFAULT 1,
    last_embedded_at TEXT,
    embedding_model TEXT
);

-- Populate embedding status for existing tasks
INSERT INTO task_embedding_status(task_id, needs_embedding)
SELECT id, 1 FROM tasks;

-- Trigger to create embedding status row on task insert
CREATE TRIGGER task_embedding_status_insert AFTER INSERT ON tasks BEGIN
    INSERT INTO task_embedding_status(task_id, needs_embedding)
    VALUES (NEW.id, 1);
END;

-- Trigger to invalidate embedding when title or description changes
CREATE TRIGGER task_embedding_invalidate AFTER UPDATE ON tasks
WHEN OLD.title != NEW.title OR OLD.description IS NOT NEW.description BEGIN
    UPDATE task_embedding_status
    SET needs_embedding = 1
    WHERE task_id = NEW.id;
END;

--------------------------------------------------------------------------------
-- sqlite-vec: Vector embeddings table (requires extension)
--------------------------------------------------------------------------------
-- NOTE: The vec0 virtual table below requires the sqlite-vec extension.
-- This must be created at runtime after loading the extension:
--   SELECT load_extension('path/to/vec0');
--
-- SQL to create the embeddings table (run after extension is loaded):
--
--   CREATE VIRTUAL TABLE task_embeddings USING vec0(
--       task_rowid INTEGER PRIMARY KEY,
--       embedding FLOAT[384]
--   );
--
-- The task_rowid should match tasks.rowid for efficient joins.
-- 384 dimensions is for BGE-small-en-v1.5 model.
