use std::{
    str::FromStr,
    sync::{Arc, Once},
    time::Duration,
};

use sqlx::{
    Error, Pool, Sqlite,
    sqlite::{
        SqliteConnectOptions, SqliteConnection, SqliteJournalMode, SqlitePoolOptions,
        SqliteSynchronous,
    },
};
use utils::assets::asset_dir;

pub mod models;

static SQLITE_VEC_INIT: Once = Once::new();
static mut SQLITE_VEC_AVAILABLE: bool = false;

/// Initializes the sqlite-vec extension globally for all SQLite connections.
/// This must be called before creating any database connections.
/// Safe to call multiple times - only the first call has effect.
///
/// Returns true if sqlite-vec was successfully initialized, false otherwise.
pub fn init_sqlite_vec() -> bool {
    SQLITE_VEC_INIT.call_once(|| {
        // Safety: sqlite3_auto_extension is called once before any connections are made.
        // The transmute is needed because sqlite-vec declares sqlite3_vec_init without
        // parameters, but it actually conforms to the SQLite extension init signature.
        unsafe {
            // sqlite3_auto_extension expects a function with the signature:
            // fn(db: *mut sqlite3, errmsg: *mut *mut c_char, api: *const sqlite3_api_routines) -> c_int
            // sqlite-vec's sqlite3_vec_init has this signature but is declared without it in Rust
            let init_fn: Option<
                unsafe extern "C" fn(
                    *mut libsqlite3_sys::sqlite3,
                    *mut *mut i8,
                    *const libsqlite3_sys::sqlite3_api_routines,
                ) -> i32,
            > = Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            ));

            let result = libsqlite3_sys::sqlite3_auto_extension(init_fn);

            if result == libsqlite3_sys::SQLITE_OK {
                SQLITE_VEC_AVAILABLE = true;
                tracing::info!("sqlite-vec extension registered successfully");
            } else {
                tracing::warn!(
                    "Failed to register sqlite-vec extension (error code: {}). Vector search will be unavailable.",
                    result
                );
            }
        }
    });

    // Safety: Only read after SQLITE_VEC_INIT has completed
    unsafe { SQLITE_VEC_AVAILABLE }
}

/// Returns whether sqlite-vec is available for use.
/// Must be called after `init_sqlite_vec()`.
pub fn is_sqlite_vec_available() -> bool {
    // Safety: Only meaningful after init_sqlite_vec() has been called
    unsafe { SQLITE_VEC_AVAILABLE }
}

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
        // Initialize sqlite-vec before creating any connections
        init_sqlite_vec();

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
        // Initialize sqlite-vec before creating any connections
        init_sqlite_vec();

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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;

    #[tokio::test]
    async fn test_sqlite_vec_available() {
        // Initialize sqlite-vec extension
        let available = init_sqlite_vec();
        assert!(available, "sqlite-vec should be available");

        // Create an in-memory database to test vec functions
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("Failed to connect to in-memory database");

        // Verify vec_version() is available
        let row = sqlx::query("SELECT vec_version()")
            .fetch_one(&pool)
            .await
            .expect("Failed to execute vec_version()");

        let version: String = row.get(0);
        assert!(
            version.starts_with('v'),
            "vec_version should return a version string starting with 'v', got: {}",
            version
        );
    }

    #[tokio::test]
    async fn test_is_sqlite_vec_available() {
        // First call init to set up
        init_sqlite_vec();

        // Then check availability
        assert!(
            is_sqlite_vec_available(),
            "is_sqlite_vec_available should return true after init"
        );
    }
}
