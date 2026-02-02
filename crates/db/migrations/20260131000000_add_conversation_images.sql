-- Create junction table for conversation_session-image associations
-- Follows the same pattern as task_images from 20250818150000_refactor_images_to_junction_tables.sql

CREATE TABLE IF NOT EXISTS conversation_images (
    id                      BLOB PRIMARY KEY,
    conversation_session_id BLOB NOT NULL,
    image_id                BLOB NOT NULL,
    created_at              TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (conversation_session_id) REFERENCES conversation_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (image_id) REFERENCES images(id) ON DELETE CASCADE,
    UNIQUE(conversation_session_id, image_id)
);

-- Create indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_conversation_images_conversation_session_id ON conversation_images(conversation_session_id);
CREATE INDEX IF NOT EXISTS idx_conversation_images_image_id ON conversation_images(image_id);
