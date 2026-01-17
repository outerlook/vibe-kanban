use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Zip error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("Backup directory error: {0}")]
    BackupDirError(String),

    #[error("Invalid filename: {0}")]
    InvalidFilename(String),

    #[error("Retention policy error: {0}")]
    RetentionError(String),
}
