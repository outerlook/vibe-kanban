use std::sync::Arc;

use anyhow::{Context, Result};
use db::models::task::Task;
use embed_anything::{
    config::TextEmbedConfig,
    embed_query,
    embeddings::embed::{EmbedData, Embedder, EmbedderBuilder, EmbeddingResult},
};

/// Default model ID for text embeddings
const DEFAULT_MODEL_ID: &str = "BAAI/bge-small-en-v1.5";

/// Service for generating text embeddings using the embed_anything crate.
/// Uses BGE-small-en-v1.5 model which produces 384-dimensional vectors.
#[derive(Clone)]
pub struct EmbeddingService {
    embedder: Arc<Embedder>,
}

impl EmbeddingService {
    /// Create a new EmbeddingService with the default BGE-small-en-v1.5 model.
    /// Downloads the model on first use to the HuggingFace cache directory.
    pub fn new() -> Result<Self> {
        let embedder = EmbedderBuilder::new()
            .model_architecture("bert")
            .model_id(Some(DEFAULT_MODEL_ID))
            .from_pretrained_hf()
            .context("Failed to load embedding model from HuggingFace")?;

        Ok(Self {
            embedder: Arc::new(embedder),
        })
    }

    /// Generate an embedding vector for a single text string.
    /// Returns a 384-dimensional vector.
    pub async fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.embed_batch(&[text.to_string()]).await?;
        embeddings
            .into_iter()
            .next()
            .context("No embedding returned for text")
    }

    /// Generate an embedding vector for a task.
    /// Combines the task title and description into a single text for embedding.
    pub async fn embed_task(&self, task: &Task) -> Result<Vec<f32>> {
        let text = format_task_text(task);
        self.embed_text(&text).await
    }

    /// Generate embeddings for multiple texts in a single batch.
    /// More efficient than calling embed_text multiple times.
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let queries: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let embedder = Arc::clone(&self.embedder);

        // embed_query is async and handles batching internally
        let config = TextEmbedConfig::default();
        let embed_data: Vec<EmbedData> = embed_query(&queries, &embedder, Some(&config))
            .await
            .context("Failed to generate embeddings")?;

        let embeddings: Vec<Vec<f32>> = embed_data
            .into_iter()
            .map(|data| extract_dense_embedding(data.embedding))
            .collect::<Result<Vec<_>>>()?;

        if embeddings.len() != texts.len() {
            anyhow::bail!(
                "Embedding count mismatch: expected {}, got {}",
                texts.len(),
                embeddings.len()
            );
        }

        Ok(embeddings)
    }
}

/// Extract dense embedding vector from EmbeddingResult.
/// Returns an error if the result is a multi-vector (not supported for this use case).
fn extract_dense_embedding(result: EmbeddingResult) -> Result<Vec<f32>> {
    match result {
        EmbeddingResult::DenseVector(vec) => Ok(vec),
        EmbeddingResult::MultiVector(_) => {
            anyhow::bail!("Multi-vector embeddings not supported, expected dense vector")
        }
    }
}

/// Format a task's title and description into a single text string for embedding.
fn format_task_text(task: &Task) -> String {
    match &task.description {
        Some(desc) if !desc.trim().is_empty() => {
            format!("{}\n\n{}", task.title, desc)
        }
        _ => task.title.clone(),
    }
}

#[cfg(test)]
mod tests {
    use db::models::embedding::EMBEDDING_DIMENSION;

    use super::*;

    #[tokio::test]
    async fn test_embed_text_dimension() {
        let service = EmbeddingService::new().expect("Failed to create EmbeddingService");
        let embedding = service
            .embed_text("hello world")
            .await
            .expect("Failed to embed text");

        assert_eq!(
            embedding.len(),
            EMBEDDING_DIMENSION,
            "Embedding dimension should be {}",
            EMBEDDING_DIMENSION
        );
    }

    #[tokio::test]
    async fn test_embed_batch() {
        let service = EmbeddingService::new().expect("Failed to create EmbeddingService");
        let texts = vec![
            "first text".to_string(),
            "second text".to_string(),
            "third text".to_string(),
        ];

        let embeddings = service
            .embed_batch(&texts)
            .await
            .expect("Failed to batch embed");

        assert_eq!(embeddings.len(), 3, "Should return 3 embeddings");
        for (i, emb) in embeddings.iter().enumerate() {
            assert_eq!(
                emb.len(),
                EMBEDDING_DIMENSION,
                "Embedding {} should have dimension {}",
                i,
                EMBEDDING_DIMENSION
            );
        }
    }

    #[tokio::test]
    async fn test_embed_empty_batch() {
        let service = EmbeddingService::new().expect("Failed to create EmbeddingService");
        let embeddings = service
            .embed_batch(&[])
            .await
            .expect("Failed to embed empty batch");

        assert!(
            embeddings.is_empty(),
            "Empty input should return empty output"
        );
    }

    #[tokio::test]
    async fn test_format_task_text() {
        use chrono::Utc;
        use uuid::Uuid;

        let task_with_desc = Task {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "Test Task".to_string(),
            description: Some("This is a description".to_string()),
            status: db::models::task::TaskStatus::Todo,
            parent_workspace_id: None,
            shared_task_id: None,
            task_group_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let text = format_task_text(&task_with_desc);
        assert_eq!(text, "Test Task\n\nThis is a description");

        let task_without_desc = Task {
            description: None,
            ..task_with_desc.clone()
        };

        let text = format_task_text(&task_without_desc);
        assert_eq!(text, "Test Task");

        let task_empty_desc = Task {
            description: Some("   ".to_string()),
            ..task_with_desc
        };

        let text = format_task_text(&task_empty_desc);
        assert_eq!(text, "Test Task");
    }
}
