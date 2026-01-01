PRAGMA foreign_keys = ON;

CREATE TABLE execution_process_normalized_entries (
    execution_id BLOB NOT NULL,
    entry_index INTEGER NOT NULL,
    entry_json TEXT NOT NULL,
    PRIMARY KEY (execution_id, entry_index),
    FOREIGN KEY (execution_id) REFERENCES execution_processes(id) ON DELETE CASCADE
);

CREATE INDEX idx_execution_process_normalized_entries_execution_id_entry_index
    ON execution_process_normalized_entries (execution_id, entry_index);
