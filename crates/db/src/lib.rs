use std::{str::FromStr, sync::Arc};

use std::time::Duration;

use sqlx::{
    Error, Pool, Sqlite,
    sqlite::{SqliteConnectOptions, SqliteConnection, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use utils::assets::asset_dir;

pub mod models;

#[derive(Clone)]
pub struct DBService {
    pub pool: Pool<Sqlite>,
}

impl DBService {
    fn pool_options() -> SqlitePoolOptions {
        SqlitePoolOptions::new()
            .max_connections(20)
            .min_connections(1)
            .idle_timeout(Duration::from_secs(300))
            .acquire_timeout(Duration::from_secs(30))
    }

    fn connect_options() -> Result<SqliteConnectOptions, Error> {
        let database_url = format!(
            "sqlite://{}",
            asset_dir().join("db.sqlite").to_string_lossy()
        );
        Ok(SqliteConnectOptions::from_str(&database_url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(30))
            .synchronous(SqliteSynchronous::Normal))
    }

    pub async fn new() -> Result<DBService, Error> {
        let pool = Self::pool_options()
            .connect_with(Self::connect_options()?)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        sqlx::query("PRAGMA optimize").execute(&pool).await?;
        Ok(DBService { pool })
    }

    pub async fn new_with_after_connect<F>(after_connect: F) -> Result<DBService, Error>
    where
        F: for<'a> Fn(
                &'a mut SqliteConnection,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), Error>> + Send + 'a>,
            > + Send
            + Sync
            + 'static,
    {
        let after_connect = Arc::new(after_connect);
        let pool = Self::pool_options()
            .after_connect(move |conn, _meta| {
                let hook = after_connect.clone();
                Box::pin(async move {
                    hook(conn).await?;
                    Ok(())
                })
            })
            .connect_with(Self::connect_options()?)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        sqlx::query("PRAGMA optimize").execute(&pool).await?;
        Ok(DBService { pool })
    }
}
