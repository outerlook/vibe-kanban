use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum UserQuestionStatus {
    Pending,
    Answered,
    Expired,
}

impl std::fmt::Display for UserQuestionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserQuestionStatus::Pending => write!(f, "pending"),
            UserQuestionStatus::Answered => write!(f, "answered"),
            UserQuestionStatus::Expired => write!(f, "expired"),
        }
    }
}

impl std::str::FromStr for UserQuestionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(UserQuestionStatus::Pending),
            "answered" => Ok(UserQuestionStatus::Answered),
            "expired" => Ok(UserQuestionStatus::Expired),
            _ => Err(format!("Invalid UserQuestionStatus: {}", s)),
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UserQuestion {
    pub id: Uuid,
    pub approval_id: String,
    pub execution_process_id: Uuid,
    pub questions: String,
    pub answers: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub answered_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateUserQuestion {
    pub approval_id: String,
    pub execution_process_id: Uuid,
    pub questions: String,
}

impl UserQuestion {
    pub async fn create(
        pool: &SqlitePool,
        data: &CreateUserQuestion,
        id: Uuid,
    ) -> Result<Self, sqlx::Error> {
        let now = Utc::now();
        let status = UserQuestionStatus::Pending.to_string();

        sqlx::query_as!(
            UserQuestion,
            r#"INSERT INTO user_questions (
                id, approval_id, execution_process_id, questions, status, created_at
               )
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING
                id as "id!: Uuid",
                approval_id as "approval_id!: String",
                execution_process_id as "execution_process_id!: Uuid",
                questions as "questions!: String",
                answers,
                status as "status!: String",
                created_at as "created_at!: DateTime<Utc>",
                answered_at as "answered_at: DateTime<Utc>""#,
            id,
            data.approval_id,
            data.execution_process_id,
            data.questions,
            status,
            now
        )
        .fetch_one(pool)
        .await
    }

    pub async fn get_by_approval_id(
        pool: &SqlitePool,
        approval_id: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            UserQuestion,
            r#"SELECT
                id as "id!: Uuid",
                approval_id as "approval_id!: String",
                execution_process_id as "execution_process_id!: Uuid",
                questions as "questions!: String",
                answers,
                status as "status!: String",
                created_at as "created_at!: DateTime<Utc>",
                answered_at as "answered_at: DateTime<Utc>"
               FROM user_questions
               WHERE approval_id = $1"#,
            approval_id
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn get_by_execution_process_id(
        pool: &SqlitePool,
        execution_process_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            UserQuestion,
            r#"SELECT
                id as "id!: Uuid",
                approval_id as "approval_id!: String",
                execution_process_id as "execution_process_id!: Uuid",
                questions as "questions!: String",
                answers,
                status as "status!: String",
                created_at as "created_at!: DateTime<Utc>",
                answered_at as "answered_at: DateTime<Utc>"
               FROM user_questions
               WHERE execution_process_id = $1
               ORDER BY created_at DESC"#,
            execution_process_id
        )
        .fetch_all(pool)
        .await
    }

    pub async fn update_answer(
        pool: &SqlitePool,
        approval_id: &str,
        answers: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        let now = Utc::now();
        let status = UserQuestionStatus::Answered.to_string();

        sqlx::query_as!(
            UserQuestion,
            r#"UPDATE user_questions
               SET answers = $1, status = $2, answered_at = $3
               WHERE approval_id = $4
               RETURNING
                id as "id!: Uuid",
                approval_id as "approval_id!: String",
                execution_process_id as "execution_process_id!: Uuid",
                questions as "questions!: String",
                answers,
                status as "status!: String",
                created_at as "created_at!: DateTime<Utc>",
                answered_at as "answered_at: DateTime<Utc>""#,
            answers,
            status,
            now,
            approval_id
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn get_pending(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        let status = UserQuestionStatus::Pending.to_string();

        sqlx::query_as!(
            UserQuestion,
            r#"SELECT
                id as "id!: Uuid",
                approval_id as "approval_id!: String",
                execution_process_id as "execution_process_id!: Uuid",
                questions as "questions!: String",
                answers,
                status as "status!: String",
                created_at as "created_at!: DateTime<Utc>",
                answered_at as "answered_at: DateTime<Utc>"
               FROM user_questions
               WHERE status = $1
               ORDER BY created_at ASC"#,
            status
        )
        .fetch_all(pool)
        .await
    }

    pub fn status_enum(&self) -> UserQuestionStatus {
        self.status.parse().unwrap_or(UserQuestionStatus::Pending)
    }
}
