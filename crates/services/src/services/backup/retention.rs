use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Duration, NaiveDateTime, Utc};
use tracing::{debug, warn};

use super::BackupError;
use crate::services::config::BackupConfig;

#[derive(Debug, Clone)]
pub struct BackupFile {
    pub path: PathBuf,
    pub timestamp: DateTime<Utc>,
}

/// Parses a backup filename to extract the timestamp.
/// Expected format: `backup_YYYYMMDD_HHMMSS.zip`
pub fn parse_backup_filename(filename: &str) -> Option<DateTime<Utc>> {
    let stem = filename.strip_suffix(".zip")?;
    let rest = stem.strip_prefix("backup_")?;

    if rest.len() != 15 || rest.chars().nth(8) != Some('_') {
        return None;
    }

    let date_part = &rest[..8];
    let time_part = &rest[9..];

    let datetime_str = format!("{date_part}{time_part}");
    let naive = NaiveDateTime::parse_from_str(&datetime_str, "%Y%m%d%H%M%S").ok()?;
    Some(DateTime::from_naive_utc_and_offset(naive, Utc))
}

/// Lists all backup files in the given directory.
pub fn list_backups(backup_dir: &Path) -> Result<Vec<BackupFile>, BackupError> {
    if !backup_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(backup_dir)?;
    let mut backups = Vec::new();

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        if let Some(timestamp) = parse_backup_filename(filename) {
            backups.push(BackupFile { path, timestamp });
        }
    }

    backups.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    Ok(backups)
}

/// Represents which GFS tier a backup belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetentionTier {
    /// Keep all backups (within retention_hours_all window)
    Hourly,
    /// Keep one per day (1 to retention_daily_days days ago)
    Daily,
    /// Keep one per week (beyond daily, up to retention_weekly_weeks weeks)
    Weekly,
    /// Keep one per month (beyond weekly, up to retention_monthly_months months)
    Monthly,
    /// Too old, mark for deletion
    Expired,
}

/// Applies the GFS retention policy to a list of backups.
/// Returns (backups to keep, backups to delete).
pub fn apply_gfs_retention(
    backups: Vec<BackupFile>,
    config: &BackupConfig,
    now: DateTime<Utc>,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    if backups.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let hourly_cutoff = now - Duration::hours(config.retention_hours_all as i64);
    let daily_cutoff = now - Duration::days(config.retention_daily_days as i64);
    let weekly_cutoff = now - Duration::weeks(config.retention_weekly_weeks as i64);
    let monthly_cutoff = now - Duration::days(config.retention_monthly_months as i64 * 30);

    let mut keep = Vec::new();
    let mut delete = Vec::new();

    // Track which periods already have a backup kept
    let mut daily_kept: HashSet<(i32, u32, u32)> = HashSet::new(); // (year, month, day)
    let mut weekly_kept: HashSet<(i32, u32)> = HashSet::new(); // (iso_year, iso_week)
    let mut monthly_kept: HashSet<(i32, u32)> = HashSet::new(); // (year, month)

    // Process backups from oldest to newest so "first of period" means earliest
    for backup in backups {
        let tier = classify_backup(&backup, hourly_cutoff, daily_cutoff, weekly_cutoff, monthly_cutoff);

        let should_keep = match tier {
            RetentionTier::Hourly => {
                debug!(
                    path = ?backup.path,
                    timestamp = %backup.timestamp,
                    "Keeping backup (hourly tier)"
                );
                true
            }
            RetentionTier::Daily => {
                let key = (
                    backup.timestamp.year(),
                    backup.timestamp.month(),
                    backup.timestamp.day(),
                );
                if daily_kept.insert(key) {
                    debug!(
                        path = ?backup.path,
                        timestamp = %backup.timestamp,
                        day = ?key,
                        "Keeping backup (daily tier - first of day)"
                    );
                    true
                } else {
                    debug!(
                        path = ?backup.path,
                        timestamp = %backup.timestamp,
                        day = ?key,
                        "Deleting backup (daily tier - not first of day)"
                    );
                    false
                }
            }
            RetentionTier::Weekly => {
                let iso_week = backup.timestamp.iso_week();
                let key = (iso_week.year(), iso_week.week());
                if weekly_kept.insert(key) {
                    debug!(
                        path = ?backup.path,
                        timestamp = %backup.timestamp,
                        week = ?key,
                        "Keeping backup (weekly tier - first of week)"
                    );
                    true
                } else {
                    debug!(
                        path = ?backup.path,
                        timestamp = %backup.timestamp,
                        week = ?key,
                        "Deleting backup (weekly tier - not first of week)"
                    );
                    false
                }
            }
            RetentionTier::Monthly => {
                let key = (backup.timestamp.year(), backup.timestamp.month());
                if monthly_kept.insert(key) {
                    debug!(
                        path = ?backup.path,
                        timestamp = %backup.timestamp,
                        month = ?key,
                        "Keeping backup (monthly tier - first of month)"
                    );
                    true
                } else {
                    debug!(
                        path = ?backup.path,
                        timestamp = %backup.timestamp,
                        month = ?key,
                        "Deleting backup (monthly tier - not first of month)"
                    );
                    false
                }
            }
            RetentionTier::Expired => {
                debug!(
                    path = ?backup.path,
                    timestamp = %backup.timestamp,
                    "Deleting backup (expired - older than monthly retention)"
                );
                false
            }
        };

        if should_keep {
            keep.push(backup.path);
        } else {
            delete.push(backup.path);
        }
    }

    (keep, delete)
}

fn classify_backup(
    backup: &BackupFile,
    hourly_cutoff: DateTime<Utc>,
    daily_cutoff: DateTime<Utc>,
    weekly_cutoff: DateTime<Utc>,
    monthly_cutoff: DateTime<Utc>,
) -> RetentionTier {
    if backup.timestamp >= hourly_cutoff {
        RetentionTier::Hourly
    } else if backup.timestamp >= daily_cutoff {
        RetentionTier::Daily
    } else if backup.timestamp >= weekly_cutoff {
        RetentionTier::Weekly
    } else if backup.timestamp >= monthly_cutoff {
        RetentionTier::Monthly
    } else {
        RetentionTier::Expired
    }
}

/// Deletes the specified backup files.
/// Returns the count of successfully deleted files.
pub fn delete_old_backups(to_delete: Vec<PathBuf>) -> Result<u32, BackupError> {
    let mut deleted_count = 0;

    for path in to_delete {
        match fs::remove_file(&path) {
            Ok(()) => {
                debug!(path = ?path, "Deleted old backup");
                deleted_count += 1;
            }
            Err(e) => {
                warn!(path = ?path, error = %e, "Failed to delete backup");
                return Err(BackupError::Io(e));
            }
        }
    }

    Ok(deleted_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Timelike};

    fn make_backup(path: &str, timestamp: DateTime<Utc>) -> BackupFile {
        BackupFile {
            path: PathBuf::from(path),
            timestamp,
        }
    }

    fn default_config() -> BackupConfig {
        BackupConfig {
            enabled: true,
            interval_hours: 6,
            retention_hours_all: 24,
            retention_daily_days: 7,
            retention_weekly_weeks: 4,
            retention_monthly_months: 12,
        }
    }

    #[test]
    fn test_parse_backup_filename_valid() {
        let result = parse_backup_filename("backup_20240115_143022.zip");
        assert!(result.is_some());
        let dt = result.unwrap();
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 15);
        assert_eq!(dt.hour(), 14);
        assert_eq!(dt.minute(), 30);
        assert_eq!(dt.second(), 22);
    }

    #[test]
    fn test_parse_backup_filename_invalid_prefix() {
        assert!(parse_backup_filename("bkp_20240115_143022.zip").is_none());
    }

    #[test]
    fn test_parse_backup_filename_invalid_extension() {
        assert!(parse_backup_filename("backup_20240115_143022.tar").is_none());
    }

    #[test]
    fn test_parse_backup_filename_invalid_format() {
        assert!(parse_backup_filename("backup_2024-01-15_14:30:22.zip").is_none());
        assert!(parse_backup_filename("backup_20240115143022.zip").is_none());
        assert!(parse_backup_filename("backup_.zip").is_none());
    }

    #[test]
    fn test_parse_backup_filename_invalid_date() {
        assert!(parse_backup_filename("backup_20241315_143022.zip").is_none());
    }

    #[test]
    fn test_gfs_empty_list() {
        let config = default_config();
        let now = Utc::now();
        let (keep, delete) = apply_gfs_retention(Vec::new(), &config, now);
        assert!(keep.is_empty());
        assert!(delete.is_empty());
    }

    #[test]
    fn test_gfs_single_backup_recent() {
        let config = default_config();
        let now = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let backup_time = now - Duration::hours(2);

        let backups = vec![make_backup("/backups/backup_20240615_100000.zip", backup_time)];

        let (keep, delete) = apply_gfs_retention(backups, &config, now);
        assert_eq!(keep.len(), 1);
        assert!(delete.is_empty());
    }

    #[test]
    fn test_gfs_hourly_tier_keeps_all() {
        let config = default_config();
        let now = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();

        // Multiple backups within the last 24 hours
        let backups = vec![
            make_backup("/backups/b1.zip", now - Duration::hours(1)),
            make_backup("/backups/b2.zip", now - Duration::hours(6)),
            make_backup("/backups/b3.zip", now - Duration::hours(12)),
            make_backup("/backups/b4.zip", now - Duration::hours(20)),
        ];

        let (keep, delete) = apply_gfs_retention(backups, &config, now);
        assert_eq!(keep.len(), 4);
        assert!(delete.is_empty());
    }

    #[test]
    fn test_gfs_daily_tier_keeps_one_per_day() {
        let config = default_config();
        let now = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();

        // Backups from 3 days ago - multiple on same calendar day
        // Use explicit timestamps to ensure they're on the same day
        let backups = vec![
            make_backup(
                "/backups/b1.zip",
                Utc.with_ymd_and_hms(2024, 6, 12, 6, 0, 0).unwrap(),
            ),
            make_backup(
                "/backups/b2.zip",
                Utc.with_ymd_and_hms(2024, 6, 12, 12, 0, 0).unwrap(),
            ),
            make_backup(
                "/backups/b3.zip",
                Utc.with_ymd_and_hms(2024, 6, 12, 18, 0, 0).unwrap(),
            ),
        ];

        let (keep, delete) = apply_gfs_retention(backups, &config, now);
        assert_eq!(keep.len(), 1);
        assert_eq!(delete.len(), 2);
        assert_eq!(keep[0], PathBuf::from("/backups/b1.zip")); // First of day kept
    }

    #[test]
    fn test_gfs_weekly_tier_keeps_one_per_week() {
        let config = default_config();
        let now = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();

        // Backups from 2 weeks ago (beyond daily retention, within weekly retention)
        // All backups in the same ISO week (week 23: 2024-06-03 Mon to 2024-06-09 Sun)
        let backups = vec![
            make_backup(
                "/backups/b1.zip",
                Utc.with_ymd_and_hms(2024, 6, 3, 10, 0, 0).unwrap(), // Monday
            ),
            make_backup(
                "/backups/b2.zip",
                Utc.with_ymd_and_hms(2024, 6, 4, 10, 0, 0).unwrap(), // Tuesday
            ),
            make_backup(
                "/backups/b3.zip",
                Utc.with_ymd_and_hms(2024, 6, 5, 10, 0, 0).unwrap(), // Wednesday
            ),
        ];

        let (keep, delete) = apply_gfs_retention(backups, &config, now);
        assert_eq!(keep.len(), 1);
        assert_eq!(delete.len(), 2);
        assert_eq!(keep[0], PathBuf::from("/backups/b1.zip")); // First of week kept
    }

    #[test]
    fn test_gfs_monthly_tier_keeps_one_per_month() {
        let config = default_config();
        let now = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();

        // Backups from 2 months ago (beyond weekly retention)
        let backups = vec![
            make_backup(
                "/backups/b1.zip",
                Utc.with_ymd_and_hms(2024, 4, 1, 10, 0, 0).unwrap(),
            ),
            make_backup(
                "/backups/b2.zip",
                Utc.with_ymd_and_hms(2024, 4, 15, 10, 0, 0).unwrap(),
            ),
            make_backup(
                "/backups/b3.zip",
                Utc.with_ymd_and_hms(2024, 4, 28, 10, 0, 0).unwrap(),
            ),
        ];

        let (keep, delete) = apply_gfs_retention(backups, &config, now);
        assert_eq!(keep.len(), 1);
        assert_eq!(delete.len(), 2);
        assert_eq!(keep[0], PathBuf::from("/backups/b1.zip")); // First of month kept
    }

    #[test]
    fn test_gfs_expired_backups_deleted() {
        let config = default_config();
        let now = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();

        // Backup from over a year ago (beyond 12 months retention)
        let backups = vec![make_backup(
            "/backups/old.zip",
            Utc.with_ymd_and_hms(2023, 1, 1, 10, 0, 0).unwrap(),
        )];

        let (keep, delete) = apply_gfs_retention(backups, &config, now);
        assert!(keep.is_empty());
        assert_eq!(delete.len(), 1);
    }

    #[test]
    fn test_gfs_mixed_tiers() {
        let config = default_config();
        let now = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();

        let backups = vec![
            // Hourly tier - all kept
            make_backup("/backups/hourly1.zip", now - Duration::hours(2)),
            make_backup("/backups/hourly2.zip", now - Duration::hours(10)),
            // Daily tier - one per day
            make_backup(
                "/backups/daily1.zip",
                now - Duration::days(2),
            ),
            make_backup(
                "/backups/daily2.zip",
                now - Duration::days(2) + Duration::hours(6),
            ),
            make_backup(
                "/backups/daily3.zip",
                now - Duration::days(3),
            ),
            // Weekly tier - one per week
            make_backup(
                "/backups/weekly1.zip",
                now - Duration::weeks(2),
            ),
            make_backup(
                "/backups/weekly2.zip",
                now - Duration::weeks(2) + Duration::days(1),
            ),
            // Monthly tier - one per month
            make_backup(
                "/backups/monthly1.zip",
                Utc.with_ymd_and_hms(2024, 3, 5, 10, 0, 0).unwrap(),
            ),
            make_backup(
                "/backups/monthly2.zip",
                Utc.with_ymd_and_hms(2024, 3, 20, 10, 0, 0).unwrap(),
            ),
        ];

        let (keep, delete) = apply_gfs_retention(backups, &config, now);

        // Expected keeps: 2 hourly + 2 daily (different days) + 1 weekly + 1 monthly = 6
        assert_eq!(keep.len(), 6);
        // Expected deletes: 1 daily duplicate + 1 weekly duplicate + 1 monthly duplicate = 3
        assert_eq!(delete.len(), 3);
    }

    #[test]
    fn test_gfs_all_backups_same_day() {
        let config = default_config();
        let now = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();

        // All backups from the same day, 3 days ago (daily tier)
        let base_time = now - Duration::days(3);
        let backups = vec![
            make_backup("/backups/b1.zip", base_time),
            make_backup("/backups/b2.zip", base_time + Duration::hours(1)),
            make_backup("/backups/b3.zip", base_time + Duration::hours(2)),
            make_backup("/backups/b4.zip", base_time + Duration::hours(3)),
            make_backup("/backups/b5.zip", base_time + Duration::hours(4)),
        ];

        let (keep, delete) = apply_gfs_retention(backups, &config, now);
        assert_eq!(keep.len(), 1);
        assert_eq!(delete.len(), 4);
        assert_eq!(keep[0], PathBuf::from("/backups/b1.zip"));
    }

    #[test]
    fn test_gfs_boundary_conditions() {
        let config = default_config();
        let now = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();

        // Test backups exactly at tier boundaries
        let backups = vec![
            // Exactly 24 hours ago - should be in hourly tier
            make_backup("/backups/at_hourly.zip", now - Duration::hours(24)),
            // Exactly 7 days ago - should be in daily tier
            make_backup("/backups/at_daily.zip", now - Duration::days(7)),
            // Exactly 4 weeks ago - should be in weekly tier
            make_backup("/backups/at_weekly.zip", now - Duration::weeks(4)),
        ];

        let (keep, delete) = apply_gfs_retention(backups, &config, now);
        assert_eq!(keep.len(), 3);
        assert!(delete.is_empty());
    }

    #[test]
    fn test_gfs_custom_config() {
        let config = BackupConfig {
            enabled: true,
            interval_hours: 1,
            retention_hours_all: 6,   // Only 6 hours of full retention
            retention_daily_days: 3,  // Only 3 days
            retention_weekly_weeks: 2, // Only 2 weeks
            retention_monthly_months: 6, // Only 6 months
        };
        let now = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();

        let backups = vec![
            // Within 6 hours - hourly tier
            make_backup("/backups/b1.zip", now - Duration::hours(3)),
            // Beyond 6 hours but within 3 days - daily tier
            make_backup("/backups/b2.zip", now - Duration::hours(12)),
            // Beyond 3 days but within 2 weeks - weekly tier
            make_backup("/backups/b3.zip", now - Duration::days(5)),
            // Beyond 2 weeks but within 6 months - monthly tier
            make_backup("/backups/b4.zip", now - Duration::days(60)),
            // Beyond 6 months - expired
            make_backup("/backups/b5.zip", now - Duration::days(200)),
        ];

        let (keep, delete) = apply_gfs_retention(backups, &config, now);
        assert_eq!(keep.len(), 4);
        assert_eq!(delete.len(), 1);
        assert!(delete.contains(&PathBuf::from("/backups/b5.zip")));
    }
}
