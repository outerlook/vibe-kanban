PRAGMA foreign_keys = ON;

-- Add conversation_session_id to notifications for linking notifications to conversations
ALTER TABLE notifications ADD COLUMN conversation_session_id BLOB REFERENCES conversation_sessions(id) ON DELETE CASCADE;

-- Create index for efficient lookups by conversation_session_id
CREATE INDEX idx_notifications_conversation_session_id ON notifications(conversation_session_id);
