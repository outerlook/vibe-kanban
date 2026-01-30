use std::{sync::Arc, time::Duration};

use chrono::Utc;
use tokio::{sync::RwLock, time::interval};
use tracing::{debug, error, info};
use utils::assets::{asset_dir, backup_dir};

use crate::services::{
    backup::{apply_gfs_retention, create_backup_archive, delete_old_backups, list_backups},
    config::Config,
};

/// Service to run periodic backups and apply retention policy.
pub struct BackupService {
    config: Arc<RwLock<Config>>,
}

impl BackupService {
    pub async fn spawn(config: Arc<RwLock<Config>>) -> tokio::task::JoinHandle<()> {
        let service = Self { config };
        tokio::spawn(async move {
            service.start().await;
        })
    }

    async fn start(&self) {
        info!("Starting backup service");

        // Use a fixed check interval (1 hour) to periodically check if backup is due
        let check_interval = Duration::from_secs(60 * 60);
        let mut interval = interval(check_interval);

        loop {
            interval.tick().await;

            let config = self.config.read().await;
            let backup_config = config.backup.clone();
            drop(config);

            if !backup_config.enabled {
                debug!("Backup is disabled, skipping cycle");
                continue;
            }

            let backup_interval =
                Duration::from_secs(backup_config.interval_hours as u64 * 60 * 60);
            let backup_dir = backup_dir();

            // Check if we need to create a new backup based on the last backup time
            match list_backups(&backup_dir) {
                Ok(backups) => {
                    let should_backup = if let Some(latest) = backups.last() {
                        let elapsed = Utc::now()
                            .signed_duration_since(latest.timestamp)
                            .to_std()
                            .unwrap_or(Duration::ZERO);
                        elapsed >= backup_interval
                    } else {
                        // No backups exist, create one
                        true
                    };

                    if !should_backup {
                        debug!(
                            "Skipping backup, last backup is recent (interval: {:?})",
                            backup_interval
                        );
                        continue;
                    }
                }
                Err(e) => {
                    error!("Failed to list backups: {}", e);
                    // Continue to create backup anyway
                }
            }

            // Create backup
            let asset_dir = asset_dir();
            match create_backup_archive(&asset_dir, &backup_dir).await {
                Ok(path) => {
                    info!("Created backup: {}", path.display());
                }
                Err(e) => {
                    error!("Failed to create backup: {}", e);
                    continue;
                }
            }

            // Apply retention policy
            match list_backups(&backup_dir) {
                Ok(backups) => {
                    let now = Utc::now();
                    let (keep, delete) = apply_gfs_retention(backups, &backup_config, now);

                    if !delete.is_empty() {
                        info!(
                            "Retention policy: keeping {} backups, deleting {}",
                            keep.len(),
                            delete.len()
                        );

                        match delete_old_backups(delete) {
                            Ok(count) => {
                                info!("Deleted {} old backups", count);
                            }
                            Err(e) => {
                                error!("Failed to delete old backups: {}", e);
                            }
                        }
                    } else {
                        debug!("Retention policy: keeping all {} backups", keep.len());
                    }
                }
                Err(e) => {
                    error!("Failed to list backups for retention: {}", e);
                }
            }
        }
    }
}
