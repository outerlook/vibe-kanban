-- Create user_questions table for persisting AskUserQuestion data
-- This allows questions to survive server restarts and executor death

CREATE TABLE user_questions (
    id                      BLOB PRIMARY KEY,
    approval_id             TEXT UNIQUE NOT NULL,
    execution_process_id    BLOB NOT NULL,
    questions               TEXT NOT NULL,  -- JSON array of QuestionData
    answers                 TEXT,           -- JSON array of QuestionAnswer, NULL until answered
    status                  TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'answered', 'expired')),
    created_at              TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    answered_at             TEXT,
    FOREIGN KEY (execution_process_id) REFERENCES execution_processes(id) ON DELETE CASCADE
);

-- Index for looking up questions by execution process
CREATE INDEX idx_user_questions_execution_process_id ON user_questions(execution_process_id);

-- Index for finding pending questions
CREATE INDEX idx_user_questions_status ON user_questions(status);
