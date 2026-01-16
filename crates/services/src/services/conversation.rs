use db::models::{
    conversation_message::{
        ConversationMessage, ConversationMessageError, CreateConversationMessage, MessageRole,
    },
    conversation_session::{
        ConversationSession, ConversationSessionError, CreateConversationSession,
    },
    execution_process::{ExecutionProcess, ExecutionProcessError},
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

/// Response when sending a message that starts execution
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SendMessageResponse {
    pub user_message: ConversationMessage,
    pub execution_process_id: Uuid,
}

#[derive(Debug, Error)]
pub enum ConversationServiceError {
    #[error(transparent)]
    Session(#[from] ConversationSessionError),
    #[error(transparent)]
    Message(#[from] ConversationMessageError),
    #[error(transparent)]
    ExecutionProcess(#[from] ExecutionProcessError),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
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

    /// Adds a user message to a conversation.
    pub async fn add_user_message(
        pool: &SqlitePool,
        conversation_session_id: Uuid,
        content: String,
    ) -> Result<ConversationMessage, ConversationServiceError> {
        let message = ConversationMessage::create(
            pool,
            CreateConversationMessage {
                conversation_session_id,
                execution_process_id: None,
                role: MessageRole::User,
                content,
                metadata: None,
            },
        )
        .await?;

        Ok(message)
    }

    /// Adds an assistant message linked to an execution process.
    pub async fn add_assistant_message(
        pool: &SqlitePool,
        conversation_session_id: Uuid,
        execution_process_id: Uuid,
        content: String,
    ) -> Result<ConversationMessage, ConversationServiceError> {
        let message = ConversationMessage::create(
            pool,
            CreateConversationMessage {
                conversation_session_id,
                execution_process_id: Some(execution_process_id),
                role: MessageRole::Assistant,
                content,
                metadata: None,
            },
        )
        .await?;

        Ok(message)
    }

    /// Get the conversation history formatted for the agent prompt.
    pub async fn get_conversation_history_for_prompt(
        pool: &SqlitePool,
        conversation_session_id: Uuid,
    ) -> Result<String, ConversationServiceError> {
        let messages =
            ConversationMessage::find_by_conversation_session_id(pool, conversation_session_id)
                .await?;

        let history = messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    MessageRole::User => "User",
                    MessageRole::Assistant => "Assistant",
                };
                format!("{}: {}", role, msg.content)
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        Ok(history)
    }

    /// Get the latest agent session ID for continuing conversation
    pub async fn get_latest_agent_session_id(
        pool: &SqlitePool,
        conversation_session_id: Uuid,
    ) -> Result<Option<String>, ConversationServiceError> {
        let agent_session_id =
            ExecutionProcess::find_latest_conversation_agent_session_id(pool, conversation_session_id)
                .await?;
        Ok(agent_session_id)
    }
}
