use std::sync::{Arc, OnceLock};

use db::models::notification::{CreateNotification, Notification, NotificationType};
use serde_json::json;
use sqlx::SqlitePool;
use tokio::sync::RwLock;
use utils;
use uuid::Uuid;

use crate::services::config::{Config, EffectiveSound, NotificationConfig, SoundFile};

/// Service for handling cross-platform notifications including sound alerts and push notifications
#[derive(Debug, Clone)]
pub struct NotificationService {
    config: Arc<RwLock<Config>>,
}

/// Cache for WSL root path from PowerShell
static WSL_ROOT_PATH_CACHE: OnceLock<Option<String>> = OnceLock::new();

impl NotificationService {
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Self { config }
    }

    /// Send both sound and push notifications if enabled
    pub async fn notify(&self, title: &str, message: &str) {
        let config = self.config.read().await.notifications.clone();
        Self::send_notification(&config, title, message).await;
    }

    /// Send error notification with error sound file
    pub async fn notify_error(&self, title: &str, message: &str) {
        let config = self.config.read().await.notifications.clone();
        if config.sound_enabled
            && let Some(path) = Self::bundled_sound_path(&config.error_sound_file).await
        {
            Self::play_sound_file(&path).await;
        }
        if config.push_enabled {
            Self::send_push_notification(title, message).await;
        }
    }

    /// Internal method to send notifications with a given config
    async fn send_notification(config: &NotificationConfig, title: &str, message: &str) {
        if config.sound_enabled {
            Self::play_sound_notification(config).await;
        }

        if config.push_enabled {
            Self::send_push_notification(title, message).await;
        }
    }

    /// Play a system sound notification across platforms
    async fn play_sound_notification(config: &NotificationConfig) {
        let file_path = match Self::resolve_sound_path(config).await {
            Some(path) => path,
            None => return,
        };

        // Use platform-specific sound notification
        // Note: spawn() calls are intentionally not awaited - sound notifications should be fire-and-forget
        if cfg!(target_os = "macos") {
            let _ = tokio::process::Command::new("afplay")
                .arg(&file_path)
                .spawn();
        } else if cfg!(target_os = "linux") && !utils::is_wsl2() {
            // Try different Linux audio players
            if tokio::process::Command::new("paplay")
                .arg(&file_path)
                .spawn()
                .is_ok()
            {
                // Success with paplay
            } else if tokio::process::Command::new("aplay")
                .arg(&file_path)
                .spawn()
                .is_ok()
            {
                // Success with aplay
            } else {
                // Try system bell as fallback
                let _ = tokio::process::Command::new("echo")
                    .arg("-e")
                    .arg("\\a")
                    .spawn();
            }
        } else if cfg!(target_os = "windows") || (cfg!(target_os = "linux") && utils::is_wsl2()) {
            // Convert WSL path to Windows path if in WSL2
            let file_path = if utils::is_wsl2() {
                if let Some(windows_path) = Self::wsl_to_windows_path(&file_path).await {
                    windows_path
                } else {
                    file_path.to_string_lossy().to_string()
                }
            } else {
                file_path.to_string_lossy().to_string()
            };

            let _ = tokio::process::Command::new("powershell.exe")
                .arg("-c")
                .arg(format!(
                    r#"(New-Object Media.SoundPlayer "{file_path}").PlaySync()"#
                ))
                .spawn();
        }
    }

    /// Play a sound file from a given path
    async fn play_sound_file(file_path: &std::path::Path) {
        // Use platform-specific sound notification
        // Note: spawn() calls are intentionally not awaited - sound notifications should be fire-and-forget
        if cfg!(target_os = "macos") {
            let _ = tokio::process::Command::new("afplay")
                .arg(file_path)
                .spawn();
        } else if cfg!(target_os = "linux") && !utils::is_wsl2() {
            // Try different Linux audio players
            if tokio::process::Command::new("paplay")
                .arg(file_path)
                .spawn()
                .is_ok()
            {
                // Success with paplay
            } else if tokio::process::Command::new("aplay")
                .arg(file_path)
                .spawn()
                .is_ok()
            {
                // Success with aplay
            } else {
                // Try system bell as fallback
                let _ = tokio::process::Command::new("echo")
                    .arg("-e")
                    .arg("\\a")
                    .spawn();
            }
        } else if cfg!(target_os = "windows") || (cfg!(target_os = "linux") && utils::is_wsl2()) {
            // Convert WSL path to Windows path if in WSL2
            let file_path_str = if utils::is_wsl2() {
                if let Some(windows_path) = Self::wsl_to_windows_path(file_path).await {
                    windows_path
                } else {
                    file_path.to_string_lossy().to_string()
                }
            } else {
                file_path.to_string_lossy().to_string()
            };

            let _ = tokio::process::Command::new("powershell.exe")
                .arg("-c")
                .arg(format!(
                    r#"(New-Object Media.SoundPlayer "{}").PlaySync()"#,
                    file_path_str
                ))
                .spawn();
        }
    }

    async fn resolve_sound_path(config: &NotificationConfig) -> Option<std::path::PathBuf> {
        match config.effective_sound() {
            EffectiveSound::Custom(filename) => {
                let custom_path = utils::assets::alerts_dir().join(&filename);
                match tokio::fs::metadata(&custom_path).await {
                    Ok(_) => Some(custom_path),
                    Err(error) => {
                        tracing::warn!("Custom sound file not found: {} ({})", filename, error);
                        Self::bundled_sound_path(&SoundFile::CowMooing).await
                    }
                }
            }
            EffectiveSound::Bundled(sound_file) => Self::bundled_sound_path(&sound_file).await,
        }
    }

    async fn bundled_sound_path(sound_file: &SoundFile) -> Option<std::path::PathBuf> {
        match sound_file.get_path().await {
            Ok(path) => Some(path),
            Err(e) => {
                tracing::error!("Failed to get cached sound file: {}", e);
                None
            }
        }
    }

    /// Send a cross-platform push notification
    async fn send_push_notification(title: &str, message: &str) {
        if cfg!(target_os = "macos") {
            Self::send_macos_notification(title, message).await;
        } else if cfg!(target_os = "linux") && !utils::is_wsl2() {
            Self::send_linux_notification(title, message).await;
        } else if cfg!(target_os = "windows") || (cfg!(target_os = "linux") && utils::is_wsl2()) {
            Self::send_windows_notification(title, message).await;
        }
    }

    /// Send macOS notification using osascript
    async fn send_macos_notification(title: &str, message: &str) {
        let script = format!(
            r#"display notification "{message}" with title "{title}" sound name "Glass""#,
            message = message.replace('"', r#"\""#),
            title = title.replace('"', r#"\""#)
        );

        let _ = tokio::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .spawn();
    }

    /// Send Linux notification using notify-rust
    async fn send_linux_notification(title: &str, message: &str) {
        use notify_rust::Notification;

        let title = title.to_string();
        let message = message.to_string();

        let _handle = tokio::task::spawn_blocking(move || {
            if let Err(e) = Notification::new()
                .summary(&title)
                .body(&message)
                .timeout(10000)
                .show()
            {
                tracing::error!("Failed to send Linux notification: {}", e);
            }
        });
        drop(_handle); // Don't await, fire-and-forget
    }

    /// Send Windows/WSL notification using PowerShell toast script
    async fn send_windows_notification(title: &str, message: &str) {
        let script_path = match utils::get_powershell_script().await {
            Ok(path) => path,
            Err(e) => {
                tracing::error!("Failed to get PowerShell script: {}", e);
                return;
            }
        };

        // Convert WSL path to Windows path if in WSL2
        let script_path_str = if utils::is_wsl2() {
            if let Some(windows_path) = Self::wsl_to_windows_path(&script_path).await {
                windows_path
            } else {
                script_path.to_string_lossy().to_string()
            }
        } else {
            script_path.to_string_lossy().to_string()
        };

        let _ = tokio::process::Command::new("powershell.exe")
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(script_path_str)
            .arg("-Title")
            .arg(title)
            .arg("-Message")
            .arg(message)
            .spawn();
    }

    /// Get WSL root path via PowerShell (cached)
    async fn get_wsl_root_path() -> Option<String> {
        if let Some(cached) = WSL_ROOT_PATH_CACHE.get() {
            return cached.clone();
        }

        match tokio::process::Command::new("powershell.exe")
            .arg("-c")
            .arg("(Get-Location).Path -replace '^.*::', ''")
            .current_dir("/")
            .output()
            .await
        {
            Ok(output) => {
                match String::from_utf8(output.stdout) {
                    Ok(pwd_str) => {
                        let pwd = pwd_str.trim();
                        tracing::info!("WSL root path detected: {}", pwd);

                        // Cache the result
                        let _ = WSL_ROOT_PATH_CACHE.set(Some(pwd.to_string()));
                        return Some(pwd.to_string());
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse PowerShell pwd output as UTF-8: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to execute PowerShell pwd command: {}", e);
            }
        }

        // Cache the failure result
        let _ = WSL_ROOT_PATH_CACHE.set(None);
        None
    }

    /// Convert WSL path to Windows UNC path for PowerShell
    async fn wsl_to_windows_path(wsl_path: &std::path::Path) -> Option<String> {
        let path_str = wsl_path.to_string_lossy();

        // Relative paths work fine as-is in PowerShell
        if !path_str.starts_with('/') {
            tracing::debug!("Using relative path as-is: {}", path_str);
            return Some(path_str.to_string());
        }

        // Get cached WSL root path from PowerShell
        if let Some(wsl_root) = Self::get_wsl_root_path().await {
            // Simply concatenate WSL root with the absolute path - PowerShell doesn't mind /
            let windows_path = format!("{wsl_root}{path_str}");
            tracing::debug!("WSL path converted: {} -> {}", path_str, windows_path);
            Some(windows_path)
        } else {
            tracing::error!(
                "Failed to determine WSL root path for conversion: {}",
                path_str
            );
            None
        }
    }

    // -------------------------------------------------------------------------
    // In-App Notification Helpers
    // -------------------------------------------------------------------------

    /// Create an in-app notification when an agent completes successfully.
    pub async fn notify_agent_complete(
        pool: &SqlitePool,
        project_id: Uuid,
        workspace_id: Uuid,
        task_title: &str,
    ) -> Result<Notification, sqlx::Error> {
        Notification::create(
            pool,
            &CreateNotification {
                project_id: Some(project_id),
                notification_type: NotificationType::AgentComplete,
                title: format!("Task Complete: {}", task_title),
                message: format!("'{}' completed successfully", task_title),
                metadata: Some(json!({ "workspace_id": workspace_id.to_string() })),
                workspace_id: Some(workspace_id),
                session_id: None,
                conversation_session_id: None,
            },
        )
        .await
    }

    /// Create an in-app notification when an agent needs approval.
    pub async fn notify_agent_approval_needed(
        pool: &SqlitePool,
        project_id: Uuid,
        workspace_id: Uuid,
        tool_name: &str,
    ) -> Result<Notification, sqlx::Error> {
        Notification::create(
            pool,
            &CreateNotification {
                project_id: Some(project_id),
                notification_type: NotificationType::AgentApprovalNeeded,
                title: "Approval Needed".to_string(),
                message: format!("Tool '{}' requires approval", tool_name),
                metadata: Some(json!({
                    "workspace_id": workspace_id.to_string(),
                    "tool_name": tool_name
                })),
                workspace_id: Some(workspace_id),
                session_id: None,
                conversation_session_id: None,
            },
        )
        .await
    }

    /// Create an in-app notification when an agent is asking questions.
    pub async fn notify_agent_question(
        pool: &SqlitePool,
        project_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<Notification, sqlx::Error> {
        Notification::create(
            pool,
            &CreateNotification {
                project_id: Some(project_id),
                notification_type: NotificationType::AgentQuestionNeeded,
                title: "Input Needed".to_string(),
                message: "Agent is asking for your input".to_string(),
                metadata: Some(json!({
                    "workspace_id": workspace_id.to_string()
                })),
                workspace_id: Some(workspace_id),
                session_id: None,
                conversation_session_id: None,
            },
        )
        .await
    }

    /// Create an in-app notification when an agent encounters an error.
    pub async fn notify_agent_error(
        pool: &SqlitePool,
        project_id: Uuid,
        workspace_id: Uuid,
        task_title: &str,
    ) -> Result<Notification, sqlx::Error> {
        Notification::create(
            pool,
            &CreateNotification {
                project_id: Some(project_id),
                notification_type: NotificationType::AgentError,
                title: format!("Task Failed: {}", task_title),
                message: format!("'{}' execution failed", task_title),
                metadata: Some(json!({ "workspace_id": workspace_id.to_string() })),
                workspace_id: Some(workspace_id),
                session_id: None,
                conversation_session_id: None,
            },
        )
        .await
    }

    /// Create an in-app notification for a new conversation response.
    pub async fn notify_conversation_response(
        pool: &SqlitePool,
        project_id: Uuid,
        conversation_session_id: Uuid,
        preview: Option<&str>,
    ) -> Result<Notification, sqlx::Error> {
        // Create preview (first 100 chars of response)
        let preview_text = preview.map(|p| {
            let trimmed = p.trim();
            if trimmed.len() > 100 {
                format!("{}...", &trimmed[..100])
            } else {
                trimmed.to_string()
            }
        });

        let message = preview_text
            .clone()
            .unwrap_or_else(|| "A conversation has received a new response".to_string());

        Notification::create(
            pool,
            &CreateNotification {
                project_id: Some(project_id),
                notification_type: NotificationType::ConversationResponse,
                title: "New Conversation Response".to_string(),
                message,
                metadata: Some(json!({
                    "conversation_session_id": conversation_session_id.to_string(),
                    "preview": preview_text
                })),
                workspace_id: None,
                session_id: None,
                conversation_session_id: Some(conversation_session_id),
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn custom_sound_resolves_to_alerts_dir() {
        let filename = format!("test-custom-{}.wav", std::process::id());
        let alerts_dir = utils::assets::alerts_dir();
        let custom_path = alerts_dir.join(&filename);

        tokio::fs::create_dir_all(&alerts_dir)
            .await
            .expect("create alerts dir");
        tokio::fs::write(&custom_path, b"test")
            .await
            .expect("write custom sound");

        let config = NotificationConfig {
            sound_enabled: true,
            push_enabled: true,
            sound_file: SoundFile::Rooster,
            error_sound_file: SoundFile::ErrorBuzzer,
            custom_sound_path: Some(filename),
        };

        let resolved = NotificationService::resolve_sound_path(&config)
            .await
            .expect("resolve sound path");

        assert_eq!(resolved, custom_path);

        let _ = tokio::fs::remove_file(&custom_path).await;
    }

    #[tokio::test]
    async fn missing_custom_sound_falls_back_to_default() {
        let filename = format!("missing-custom-{}.wav", std::process::id());
        let config = NotificationConfig {
            sound_enabled: true,
            push_enabled: true,
            sound_file: SoundFile::Rooster,
            error_sound_file: SoundFile::ErrorBuzzer,
            custom_sound_path: Some(filename),
        };

        let resolved = NotificationService::resolve_sound_path(&config)
            .await
            .expect("resolve sound path");
        let default_path = SoundFile::CowMooing
            .get_path()
            .await
            .expect("default sound path");

        assert_eq!(resolved, default_path);
    }
}
