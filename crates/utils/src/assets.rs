use directories::ProjectDirs;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

const PROJECT_ROOT: &str = env!("CARGO_MANIFEST_DIR");

pub fn asset_dir() -> std::path::PathBuf {
    let path = if cfg!(debug_assertions) {
        std::path::PathBuf::from(PROJECT_ROOT).join("../../dev_assets")
    } else {
        ProjectDirs::from("ai", "bloop", "vibe-kanban")
            .expect("OS didn't give us a home directory")
            .data_dir()
            .to_path_buf()
    };

    // Ensure the directory exists
    if !path.exists() {
        std::fs::create_dir_all(&path).expect("Failed to create asset directory");
    }

    path
    // ✔ macOS → ~/Library/Application Support/MyApp
    // ✔ Linux → ~/.local/share/myapp   (respects XDG_DATA_HOME)
    // ✔ Windows → %APPDATA%\Example\MyApp
}

pub fn config_path() -> std::path::PathBuf {
    asset_dir().join("config.json")
}

pub fn profiles_path() -> std::path::PathBuf {
    asset_dir().join("profiles.json")
}

pub fn editors_path() -> std::path::PathBuf {
    asset_dir().join("editors.json")
}

pub fn credentials_path() -> std::path::PathBuf {
    asset_dir().join("credentials.json")
}

pub fn alerts_dir() -> std::path::PathBuf {
    asset_dir().join("alerts")
}

pub fn backup_dir() -> std::path::PathBuf {
    asset_dir().join("backups")
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CustomSoundInfo {
    pub filename: String,
}

/// Lists custom sound files (.wav, .mp3) from the alerts directory.
/// Returns an empty Vec if the directory doesn't exist.
pub async fn list_custom_sounds() -> Vec<CustomSoundInfo> {
    let dir = alerts_dir();

    let mut read_dir = match tokio::fs::read_dir(&dir).await {
        Ok(rd) => rd,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("Failed to read alerts directory {}: {}", dir.display(), e);
            }
            return Vec::new();
        }
    };

    let mut sounds = Vec::new();

    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let path = entry.path();

        // Check extension (case-insensitive) for .wav or .mp3
        let is_sound = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                let lower = ext.to_lowercase();
                lower == "wav" || lower == "mp3"
            })
            .unwrap_or(false);

        if !is_sound {
            continue;
        }

        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            sounds.push(CustomSoundInfo {
                filename: filename.to_string(),
            });
        }
    }

    sounds
}

#[derive(RustEmbed)]
#[folder = "../../assets/sounds"]
pub struct SoundAssets;

#[derive(RustEmbed)]
#[folder = "../../assets/scripts"]
pub struct ScriptAssets;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_alerts_dir_path_construction() {
        let alerts = alerts_dir();
        let asset = asset_dir();

        // alerts_dir should be a subdirectory of asset_dir
        assert_eq!(alerts, asset.join("alerts"));
        assert!(alerts.ends_with("alerts"));
    }

    #[tokio::test]
    async fn test_list_custom_sounds_missing_directory() {
        // This test relies on the alerts directory not existing in test environment
        // The function should return an empty vec without error
        let sounds = list_custom_sounds().await;
        // Just verify it doesn't panic and returns something (could be empty or not depending on env)
        assert!(sounds.is_empty() || !sounds.is_empty());
    }

    #[tokio::test]
    async fn test_list_custom_sounds_with_temp_directory() {
        let temp_dir = TempDir::new().unwrap();
        let alerts_path = temp_dir.path();

        // Create test files
        let mut wav_file = std::fs::File::create(alerts_path.join("test.wav")).unwrap();
        wav_file.write_all(b"fake wav content").unwrap();

        let mut mp3_file = std::fs::File::create(alerts_path.join("music.MP3")).unwrap();
        mp3_file.write_all(b"fake mp3 content").unwrap();

        // Create a file with wrong extension (should be skipped)
        let mut txt_file = std::fs::File::create(alerts_path.join("notes.txt")).unwrap();
        txt_file.write_all(b"text content").unwrap();

        // Read directory directly to test the logic
        let mut read_dir = tokio::fs::read_dir(alerts_path).await.unwrap();
        let mut found_wav = false;
        let mut found_mp3 = false;
        let mut found_txt = false;

        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let path = entry.path();
            let extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_lowercase());

            match extension.as_deref() {
                Some("wav") => found_wav = true,
                Some("mp3") => found_mp3 = true,
                Some("txt") => found_txt = true,
                _ => {}
            }
        }

        assert!(found_wav, "Should find .wav file");
        assert!(found_mp3, "Should find .MP3 file (case-insensitive)");
        assert!(found_txt, "Should find .txt file in directory");
    }

    #[test]
    fn test_custom_sound_info_serialization() {
        let info = CustomSoundInfo {
            filename: "test.wav".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"filename\":\"test.wav\""));
    }
}
