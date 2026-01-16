use std::time::Duration;

use db::{
    DBService,
    models::{
        embedding::{EmbeddingStatus, TaskEmbedding},
        task::Task,
    },
};
use tokio::time::interval;
use tracing::{debug, error, info};

use crate::services::embedding::EmbeddingService;

/// Default model name used for tracking which model generated embeddings.
const EMBEDDING_MODEL_NAME: &str = "BAAI/bge-small-en-v1.5";

/// Background worker that polls for tasks needing embeddings and processes them.
/// Runs in its own tokio task and processes embeddings asynchronously.
pub struct EmbeddingWorker {
    embedding_service: EmbeddingService,
    db: DBService,
    poll_interval: Duration,
    batch_size: i64,
}

impl EmbeddingWorker {
    /// Create a new EmbeddingWorker with custom settings.
    pub fn new(
        embedding_service: EmbeddingService,
        db: DBService,
        poll_interval: Duration,
        batch_size: i64,
    ) -> Self {
        Self {
            embedding_service,
            db,
            poll_interval,
            batch_size,
        }
    }

    /// Spawn the embedding worker as a background task.
    /// Returns a JoinHandle that can be used to await or abort the worker.
    pub fn spawn(
        embedding_service: EmbeddingService,
        db: DBService,
    ) -> tokio::task::JoinHandle<()> {
        let worker = Self::new(
            embedding_service,
            db,
            Duration::from_secs(30), // Default 30s poll interval
            50,                      // Default batch size
        );
        tokio::spawn(async move {
            worker.run().await;
        })
    }

    /// Run the worker loop indefinitely.
    async fn run(&self) {
        info!(
            "Starting EmbeddingWorker with poll_interval={:?}, batch_size={}",
            self.poll_interval, self.batch_size
        );

        // Ensure the task_embeddings table exists
        match TaskEmbedding::ensure_table_exists(&self.db.pool).await {
            Ok(created) => {
                if created {
                    info!("Created task_embeddings virtual table");
                } else {
                    debug!("task_embeddings table already exists");
                }
            }
            Err(e) => {
                error!("Failed to ensure task_embeddings table exists: {}", e);
                // Continue anyway - the table might already exist or be created later
            }
        }

        let mut ticker = interval(self.poll_interval);

        loop {
            ticker.tick().await;
            if let Err(e) = self.process_pending_embeddings().await {
                error!("Error processing pending embeddings: {}", e);
            }
        }
    }

    /// Process a batch of pending embedding tasks.
    async fn process_pending_embeddings(&self) -> Result<(), anyhow::Error> {
        let pending = EmbeddingStatus::find_pending(&self.db.pool, self.batch_size).await?;

        if pending.is_empty() {
            debug!("No pending embeddings to process");
            return Ok(());
        }

        info!("Processing {} pending embeddings", pending.len());

        for status in pending {
            if let Err(e) = self.process_single_task(status.task_id).await {
                error!("Failed to embed task {}: {}", status.task_id, e);
                // Continue processing other tasks
            }
        }

        Ok(())
    }

    /// Process embedding for a single task.
    async fn process_single_task(&self, task_id: uuid::Uuid) -> Result<(), anyhow::Error> {
        // Fetch the task
        let task = Task::find_by_id(&self.db.pool, task_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", task_id))?;

        // Get the task's rowid for the embeddings table
        let rowid = TaskEmbedding::get_task_rowid(&self.db.pool, task_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Could not get rowid for task {}", task_id))?;

        // Generate embedding
        let embedding = self.embedding_service.embed_task(&task).await?;

        // Store the embedding
        TaskEmbedding::upsert(&self.db.pool, rowid, &embedding).await?;

        // Mark as embedded
        EmbeddingStatus::mark_embedded(&self.db.pool, task_id, EMBEDDING_MODEL_NAME).await?;

        debug!("Successfully embedded task {}", task_id);

        Ok(())
    }
}
