use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use tracing::warn;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

use super::BackupError;

const BUFFER_SIZE: usize = 8 * 1024;

const ROOT_FILES: &[&str] = &[
    "db.sqlite",
    "db.sqlite-wal",
    "db.sqlite-shm",
    "config.json",
    "profiles.json",
    "editors.json",
    "credentials.json",
];

const ALERT_DIR: &str = "alerts";

/// Creates a backup archive containing database, config files, and custom sounds.
///
/// Returns the path to the created ZIP file.
pub async fn create_backup_archive(
    asset_dir: &Path,
    backup_dir: &Path,
) -> Result<PathBuf, BackupError> {
    let asset_dir = asset_dir.to_path_buf();
    let backup_dir = backup_dir.to_path_buf();

    tokio::task::spawn_blocking(move || create_backup_archive_blocking(&asset_dir, &backup_dir))
        .await
        .map_err(|e| BackupError::BackupDirError(format!("Task join error: {e}")))?
}

fn create_backup_archive_blocking(
    asset_dir: &Path,
    backup_dir: &Path,
) -> Result<PathBuf, BackupError> {
    std::fs::create_dir_all(backup_dir)?;

    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("backup_{timestamp}.zip");
    let archive_path = backup_dir.join(&filename);

    let file = File::create(&archive_path)?;
    let mut zip = zip::ZipWriter::new(file);

    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for file_name in ROOT_FILES {
        let file_path = asset_dir.join(file_name);
        if file_path.exists() {
            add_file_to_zip(&mut zip, &file_path, file_name, options)?;
        } else {
            warn!("Skipping missing file: {}", file_path.display());
        }
    }

    let alerts_dir = asset_dir.join(ALERT_DIR);
    if alerts_dir.exists() && alerts_dir.is_dir() {
        add_directory_to_zip(&mut zip, &alerts_dir, ALERT_DIR, options)?;
    } else {
        warn!("Skipping missing alerts directory: {}", alerts_dir.display());
    }

    zip.finish()?;

    Ok(archive_path)
}

fn add_file_to_zip<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    file_path: &Path,
    archive_name: &str,
    options: SimpleFileOptions,
) -> Result<(), BackupError> {
    let file = File::open(file_path)?;
    let mut reader = BufReader::new(file);
    let mut buffer = [0u8; BUFFER_SIZE];

    zip.start_file(archive_name, options)?;

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        zip.write_all(&buffer[..bytes_read])?;
    }

    Ok(())
}

fn add_directory_to_zip<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    dir_path: &Path,
    archive_prefix: &str,
    options: SimpleFileOptions,
) -> Result<(), BackupError> {
    for entry in std::fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry
            .file_name()
            .into_string()
            .map_err(|_| BackupError::InvalidFilename(path.display().to_string()))?;
        let archive_name = format!("{archive_prefix}/{file_name}");

        if path.is_file() {
            add_file_to_zip(zip, &path, &archive_name, options)?;
        } else if path.is_dir() {
            add_directory_to_zip(zip, &path, &archive_name, options)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_create_backup_archive() {
        let asset_dir = TempDir::new().unwrap();
        let backup_dir = TempDir::new().unwrap();

        std::fs::write(asset_dir.path().join("db.sqlite"), b"test database content").unwrap();
        std::fs::write(asset_dir.path().join("config.json"), b"{}").unwrap();

        let alerts_dir = asset_dir.path().join("alerts");
        std::fs::create_dir(&alerts_dir).unwrap();
        std::fs::write(alerts_dir.join("sound1.mp3"), b"fake mp3 data").unwrap();
        std::fs::write(alerts_dir.join("sound2.wav"), b"fake wav data").unwrap();

        let archive_path = create_backup_archive(asset_dir.path(), backup_dir.path())
            .await
            .unwrap();

        assert!(archive_path.exists());
        assert!(archive_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("backup_"));
        assert!(archive_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with(".zip"));

        let file = File::open(&archive_path).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();

        let mut names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();
        names.sort();

        assert!(names.contains(&"db.sqlite".to_string()));
        assert!(names.contains(&"config.json".to_string()));
        assert!(names.contains(&"alerts/sound1.mp3".to_string()));
        assert!(names.contains(&"alerts/sound2.wav".to_string()));

        let mut db_file = zip.by_name("db.sqlite").unwrap();
        let mut contents = Vec::new();
        db_file.read_to_end(&mut contents).unwrap();
        assert_eq!(contents, b"test database content");
    }

    #[tokio::test]
    async fn test_handles_missing_optional_files() {
        let asset_dir = TempDir::new().unwrap();
        let backup_dir = TempDir::new().unwrap();

        std::fs::write(asset_dir.path().join("config.json"), b"{}").unwrap();

        let archive_path = create_backup_archive(asset_dir.path(), backup_dir.path())
            .await
            .unwrap();

        assert!(archive_path.exists());

        let file = File::open(&archive_path).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();

        let names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();

        assert!(names.contains(&"config.json".to_string()));
        assert!(!names.contains(&"db.sqlite".to_string()));
        assert!(!names.iter().any(|n| n.starts_with("alerts/")));
    }

    #[tokio::test]
    async fn test_creates_valid_extractable_zip() {
        let asset_dir = TempDir::new().unwrap();
        let backup_dir = TempDir::new().unwrap();
        let extract_dir = TempDir::new().unwrap();

        std::fs::write(asset_dir.path().join("db.sqlite"), b"database").unwrap();
        std::fs::write(asset_dir.path().join("profiles.json"), b"[]").unwrap();

        let alerts_dir = asset_dir.path().join("alerts");
        std::fs::create_dir(&alerts_dir).unwrap();
        std::fs::write(alerts_dir.join("beep.wav"), b"beep").unwrap();

        let archive_path = create_backup_archive(asset_dir.path(), backup_dir.path())
            .await
            .unwrap();

        let file = File::open(&archive_path).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        zip.extract(extract_dir.path()).unwrap();

        assert_eq!(
            std::fs::read(extract_dir.path().join("db.sqlite")).unwrap(),
            b"database"
        );
        assert_eq!(
            std::fs::read(extract_dir.path().join("profiles.json")).unwrap(),
            b"[]"
        );
        assert_eq!(
            std::fs::read(extract_dir.path().join("alerts/beep.wav")).unwrap(),
            b"beep"
        );
    }
}
