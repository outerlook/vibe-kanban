use executors::logs::NormalizedEntry;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
struct ExecutionProcessNormalizedEntryRow {
    entry_index: i64,
    entry_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionProcessNormalizedEntry {
    pub entry_index: i64,
    pub entry: NormalizedEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionProcessNormalizedEntriesPage {
    pub entries: Vec<ExecutionProcessNormalizedEntry>,
    pub next_before_index: Option<i64>,
    pub has_more: bool,
}

impl ExecutionProcessNormalizedEntry {
    pub async fn upsert(
        pool: &SqlitePool,
        execution_id: Uuid,
        entry_index: i64,
        entry: &NormalizedEntry,
    ) -> Result<(), anyhow::Error> {
        let entry_json = serde_json::to_string(entry)?;
        sqlx::query!(
            r#"INSERT INTO execution_process_normalized_entries (
                    execution_id,
                    entry_index,
                    entry_json
                ) VALUES ($1, $2, $3)
                ON CONFLICT(execution_id, entry_index)
                DO UPDATE SET entry_json = excluded.entry_json"#,
            execution_id,
            entry_index,
            entry_json
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn delete(
        pool: &SqlitePool,
        execution_id: Uuid,
        entry_index: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"DELETE FROM execution_process_normalized_entries
               WHERE execution_id = $1 AND entry_index = $2"#,
            execution_id,
            entry_index
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn count_by_execution_id(
        pool: &SqlitePool,
        execution_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        let row = sqlx::query!(
            r#"SELECT COUNT(1) as "count!: i64"
               FROM execution_process_normalized_entries
               WHERE execution_id = $1"#,
            execution_id
        )
        .fetch_one(pool)
        .await?;

        Ok(row.count)
    }

    pub async fn fetch_page(
        pool: &SqlitePool,
        execution_id: Uuid,
        before_index: Option<i64>,
        limit: usize,
    ) -> Result<ExecutionProcessNormalizedEntriesPage, anyhow::Error> {
        let limit = limit.clamp(1, 500) as i64;
        let fetch_limit = limit + 1;

        let rows: Vec<ExecutionProcessNormalizedEntryRow> = if let Some(before_index) = before_index {
            sqlx::query_as!(
                ExecutionProcessNormalizedEntryRow,
                r#"SELECT
                        entry_index,
                        entry_json
                   FROM execution_process_normalized_entries
                   WHERE execution_id = $1
                     AND entry_index < $2
                   ORDER BY entry_index DESC
                   LIMIT $3"#,
                execution_id,
                before_index,
                fetch_limit
            )
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as!(
                ExecutionProcessNormalizedEntryRow,
                r#"SELECT
                        entry_index,
                        entry_json
                   FROM execution_process_normalized_entries
                   WHERE execution_id = $1
                   ORDER BY entry_index DESC
                   LIMIT $2"#,
                execution_id,
                fetch_limit
            )
            .fetch_all(pool)
            .await?
        };

        let has_more = rows.len() as i64 > limit;
        let rows = if has_more {
            rows[..limit as usize].to_vec()
        } else {
            rows
        };

        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            let entry = serde_json::from_str(&row.entry_json)?;
            entries.push(ExecutionProcessNormalizedEntry {
                entry_index: row.entry_index,
                entry,
            });
        }

        entries.reverse();
        let next_before_index = if has_more {
            entries.first().map(|entry| entry.entry_index)
        } else {
            None
        };

        Ok(ExecutionProcessNormalizedEntriesPage {
            entries,
            next_before_index,
            has_more,
        })
    }

    pub async fn fetch_all_for_execution(
        pool: &SqlitePool,
        execution_id: Uuid,
    ) -> Result<Vec<ExecutionProcessNormalizedEntry>, anyhow::Error> {
        let rows: Vec<ExecutionProcessNormalizedEntryRow> = sqlx::query_as!(
            ExecutionProcessNormalizedEntryRow,
            r#"SELECT
                    entry_index,
                    entry_json
               FROM execution_process_normalized_entries
               WHERE execution_id = $1
               ORDER BY entry_index ASC"#,
            execution_id
        )
        .fetch_all(pool)
        .await?;

        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            let entry = serde_json::from_str(&row.entry_json)?;
            entries.push(ExecutionProcessNormalizedEntry {
                entry_index: row.entry_index,
                entry,
            });
        }

        Ok(entries)
    }
}
