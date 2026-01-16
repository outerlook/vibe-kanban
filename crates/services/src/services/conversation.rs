use db::models::{
    conversation_message::{ConversationMessage, ConversationMessageError, CreateConversationMessage, MessageRole},
    conversation_session::{ConversationSession, ConversationSessionError, CreateConversationSession},
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ConversationWithMessages {
    #[serde(flatten)]
    pub session: ConversationSession,
    pub messages: Vec<ConversationMessage>,
}

#[derive(Debug, Error)]
pub enum ConversationServiceError {
    #[error(transparent)]
    Session(#[from] ConversationSessionError),
    #[error(transparent)]
    Message(#[from] ConversationMessageError),
    #[error("Conversation not found")]
    NotFound,
}

pub struct ConversationService;

impl ConversationService {
    /// Creates a new conversation session with an initial user message.
    pub async fn create_conversation(
        pool: &SqlitePool,
        project_id: Uuid,
        title: String,
        initial_message: String,
        executor: Option<String>,
    ) -> Result<(ConversationSession, ConversationMessage), ConversationServiceError> {
        let session = ConversationSession::create(
            pool,
            CreateConversationSession {
                project_id,
                title,
                executor,
            },
        )
        .await?;

        let message = ConversationMessage::create(
            pool,
            CreateConversationMessage {
                conversation_session_id: session.id,
                execution_process_id: None,
                role: MessageRole::User,
                content: initial_message,
                metadata: None,
            },
        )
        .await?;

        Ok((session, message))
    }

    /// Retrieves a conversation session with all its messages.
    pub async fn get_conversation_with_messages(
        pool: &SqlitePool,
        conversation_session_id: Uuid,
    ) -> Result<ConversationWithMessages, ConversationServiceError> {
        let session = ConversationSession::find_by_id(pool, conversation_session_id)
            .await?
            .ok_or(ConversationServiceError::NotFound)?;

        let messages =
            ConversationMessage::find_by_conversation_session_id(pool, conversation_session_id)
                .await?;

        Ok(ConversationWithMessages { session, messages })
    }
}
