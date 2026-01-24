PRAGMA foreign_keys = ON;

-- Delete existing notifications with 'conversation_response' type
-- These have invalid FK data (conversation_session_id stored in session_id column)
DELETE FROM notifications WHERE notification_type = 'conversation_response';

-- Add conversation_session_id column with FK to conversation_sessions
ALTER TABLE notifications ADD COLUMN conversation_session_id BLOB REFERENCES conversation_sessions(id) ON DELETE CASCADE;

-- Add index for efficient querying
CREATE INDEX idx_notifications_conversation_session_id ON notifications(conversation_session_id);
