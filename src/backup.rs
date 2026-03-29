use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::{Read, Write};
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;

use crate::config::BackupConfig;
use crate::db::Database;

fn new_runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to create tokio runtime")
}

/// Run an async block on a tokio runtime, catching any panics (e.g. from invalid AWS SDK config)
fn block_on_safe<F, T>(rt: &tokio::runtime::Runtime, fut: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    std::panic::catch_unwind(AssertUnwindSafe(|| rt.block_on(fut)))
        .unwrap_or_else(|e| {
            let msg = e
                .downcast_ref::<String>()
                .map(|s| s.as_str())
                .or_else(|| e.downcast_ref::<&str>().copied())
                .unwrap_or("unknown error");
            Err(anyhow::anyhow!("Backup operation failed: {}", msg))
        })
}

#[derive(Debug, Clone)]
pub struct BackupEntry {
    pub key: String,
    pub size: i64,
    pub timestamp: DateTime<Utc>,
    pub display_name: String,
}

/// Create a compressed backup of the database and upload to S3
pub fn create_backup(config: &BackupConfig) -> Result<String> {
    let db_path = Database::db_path()?;
    if !db_path.exists() {
        anyhow::bail!("Database file not found: {}", db_path.display());
    }

    // Read and compress
    let data = std::fs::read(&db_path)
        .context("Failed to read database file")?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&data)?;
    let compressed = encoder.finish()?;

    // Generate key
    let now = Utc::now();
    let timestamp = now.format("%Y-%m-%dT%H%M%SZ");
    let key = format!("{}dodo-{}.db.gz", config.prefix, timestamp);

    // Upload
    let rt = new_runtime()?;

    block_on_safe(&rt, async {
        let s3_client = build_s3_client(config).await?;
        let bucket = config.bucket.as_deref().context("No bucket configured")?;

        s3_client
            .put_object()
            .bucket(bucket)
            .key(&key)
            .body(compressed.into())
            .send()
            .await
            .context("Failed to upload backup")?;

        Ok::<(), anyhow::Error>(())
    })?;

    // Auto-prune
    if config.max_backups > 0 {
        if let Err(e) = prune_backups(config, config.max_backups as usize) {
            eprintln!("Warning: failed to prune old backups: {e}");
        }
    }

    Ok(key)
}

/// List backups from S3
pub fn list_backups(config: &BackupConfig) -> Result<Vec<BackupEntry>> {
    let rt = new_runtime()?;

    block_on_safe(&rt, async {
        let s3_client = build_s3_client(config).await?;
        let bucket = config.bucket.as_deref().context("No bucket configured")?;

        let resp = s3_client
            .list_objects_v2()
            .bucket(bucket)
            .prefix(&config.prefix)
            .send()
            .await
            .context("Failed to list backups")?;

        let mut entries = Vec::new();
        for obj in resp.contents() {
            let key = match obj.key() {
                Some(k) => k,
                None => continue,
            };
            if !key.ends_with(".db.gz") {
                continue;
            }
            let size = obj.size().unwrap_or(0);
            let timestamp = parse_backup_timestamp(key)
                .unwrap_or_else(Utc::now);
            let display_name = key
                .rsplit('/')
                .next()
                .unwrap_or(key)
                .to_string();

            entries.push(BackupEntry {
                key: key.to_string(),
                size,
                timestamp,
                display_name,
            });
        }

        // Sort newest first
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(entries)
    })
}

/// Download and restore a backup from S3
pub fn restore_backup(config: &BackupConfig, key: &str) -> Result<()> {
    let db_path = Database::db_path()?;

    // Download from S3
    let rt = new_runtime()?;

    let compressed = block_on_safe(&rt, async {
        let s3_client = build_s3_client(config).await?;
        let bucket = config.bucket.as_deref().context("No bucket configured")?;

        let resp = s3_client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .context("Failed to download backup")?;

        let body = resp.body.collect().await
            .context("Failed to read backup body")?;
        Ok::<Vec<u8>, anyhow::Error>(body.into_bytes().to_vec())
    })?;

    // Decompress
    let mut decoder = GzDecoder::new(&compressed[..]);
    let mut restored_data = Vec::new();
    decoder.read_to_end(&mut restored_data)
        .context("Failed to decompress backup")?;

    // Safety net: save current DB as .pre-restore
    if db_path.exists() {
        let pre_restore = db_path.with_extension("db.pre-restore");
        std::fs::copy(&db_path, &pre_restore)
            .context("Failed to create pre-restore backup")?;
    }

    // Write restored data
    std::fs::write(&db_path, restored_data)
        .context("Failed to write restored database")?;

    Ok(())
}

/// Delete a specific backup from S3
pub fn delete_backup(config: &BackupConfig, key: &str) -> Result<()> {
    let rt = new_runtime()?;

    block_on_safe(&rt, async {
        let s3_client = build_s3_client(config).await?;
        let bucket = config.bucket.as_deref().context("No bucket configured")?;

        s3_client
            .delete_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .context("Failed to delete backup")?;

        Ok::<(), anyhow::Error>(())
    })
}

/// Prune old backups beyond the limit
fn prune_backups(config: &BackupConfig, max: usize) -> Result<()> {
    let entries = list_backups(config)?;
    if entries.len() <= max {
        return Ok(());
    }

    // entries are sorted newest-first, so skip the first `max` and delete the rest
    for entry in entries.iter().skip(max) {
        delete_backup(config, &entry.key)?;
    }

    Ok(())
}

/// Check if backup is overdue (for startup warning)
pub fn check_backup_age(config: &BackupConfig) -> Result<Option<u64>> {
    if !config.is_ready() || config.schedule_days == 0 {
        return Ok(None);
    }

    let entries = list_backups(config)?;
    if entries.is_empty() {
        return Ok(Some(u64::MAX)); // Never backed up
    }

    let latest = &entries[0];
    let age_days = (Utc::now() - latest.timestamp).num_days() as u64;
    if age_days >= config.schedule_days as u64 {
        Ok(Some(age_days))
    } else {
        Ok(None)
    }
}

/// Format bytes into human-readable size
pub fn format_size(bytes: i64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Format relative time (e.g. "2 days ago")
pub fn format_age(timestamp: &DateTime<Utc>) -> String {
    let diff = Utc::now() - *timestamp;
    let days = diff.num_days();
    if days == 0 {
        let hours = diff.num_hours();
        if hours == 0 {
            "just now".to_string()
        } else {
            format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
        }
    } else if days == 1 {
        "yesterday".to_string()
    } else {
        format!("{} days ago", days)
    }
}

// Parse timestamp from backup key like "dodo/dodo-2026-02-12T030000Z.db.gz"
fn parse_backup_timestamp(key: &str) -> Option<DateTime<Utc>> {
    let name = key.rsplit('/').next()?;
    let name = name.strip_prefix("dodo-")?;
    let name = name.strip_suffix(".db.gz")?;
    // name = "2026-02-12T030000Z" — strip trailing Z and parse as NaiveDateTime
    let name = name.strip_suffix('Z')?;
    chrono::NaiveDateTime::parse_from_str(name, "%Y-%m-%dT%H%M%S")
        .ok()
        .map(|d| d.and_utc())
}

async fn build_s3_client(config: &BackupConfig) -> Result<aws_sdk_s3::Client> {
    let endpoint = config.endpoint.as_deref().context("No endpoint configured")?;
    let access_key = config.access_key.as_deref().context("No access key configured")?;
    let secret_key = config.secret_key.as_deref().context("No secret key configured")?;
    let region = config.region.as_deref().unwrap_or("us-east-1");

    let creds = aws_sdk_s3::config::Credentials::new(
        access_key,
        secret_key,
        None,
        None,
        "dodo",
    );

    let s3_config = aws_sdk_s3::Config::builder()
        .behavior_version_latest()
        .endpoint_url(endpoint)
        .region(aws_sdk_s3::config::Region::new(region.to_string()))
        .credentials_provider(creds)
        .force_path_style(true)
        .build();

    Ok(aws_sdk_s3::Client::from_conf(s3_config))
}

/// Test S3 connection by listing backups
pub fn test_connection(config: &BackupConfig) -> Result<String> {
    let entries = list_backups(config)?;
    Ok(format!("Connected — {} backup(s) found", entries.len()))
}

/// Get the path to the pre-restore backup
pub fn pre_restore_path() -> Result<PathBuf> {
    let db_path = Database::db_path()?;
    Ok(db_path.with_extension("db.pre-restore"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    // ── format_size ─────────────────────────────────────────────────

    #[test]
    fn format_size_zero_bytes() {
        assert_eq!(format_size(0), "0 B");
    }

    #[test]
    fn format_size_small_bytes() {
        assert_eq!(format_size(512), "512 B");
    }

    #[test]
    fn format_size_just_under_kb() {
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn format_size_exact_kb() {
        assert_eq!(format_size(1024), "1.0 KB");
    }

    #[test]
    fn format_size_fractional_kb() {
        assert_eq!(format_size(1536), "1.5 KB");
    }

    #[test]
    fn format_size_large_kb() {
        assert_eq!(format_size(512_000), "500.0 KB");
    }

    #[test]
    fn format_size_just_under_mb() {
        assert_eq!(format_size(1_048_575), "1024.0 KB");
    }

    #[test]
    fn format_size_exact_mb() {
        assert_eq!(format_size(1_048_576), "1.0 MB");
    }

    #[test]
    fn format_size_large_mb() {
        assert_eq!(format_size(10_485_760), "10.0 MB");
    }

    #[test]
    fn format_size_fractional_mb() {
        assert_eq!(format_size(1_572_864), "1.5 MB");
    }

    // ── format_age ──────────────────────────────────────────────────

    #[test]
    fn format_age_just_now() {
        let ts = Utc::now() - Duration::seconds(30);
        assert_eq!(format_age(&ts), "just now");
    }

    #[test]
    fn format_age_one_hour() {
        let ts = Utc::now() - Duration::hours(1);
        assert_eq!(format_age(&ts), "1 hour ago");
    }

    #[test]
    fn format_age_multiple_hours() {
        let ts = Utc::now() - Duration::hours(5);
        assert_eq!(format_age(&ts), "5 hours ago");
    }

    #[test]
    fn format_age_yesterday() {
        let ts = Utc::now() - Duration::days(1);
        assert_eq!(format_age(&ts), "yesterday");
    }

    #[test]
    fn format_age_two_days() {
        let ts = Utc::now() - Duration::days(2);
        assert_eq!(format_age(&ts), "2 days ago");
    }

    #[test]
    fn format_age_many_days() {
        let ts = Utc::now() - Duration::days(30);
        assert_eq!(format_age(&ts), "30 days ago");
    }

    // ── parse_backup_timestamp ──────────────────────────────────────

    #[test]
    fn parse_timestamp_valid_key() {
        let ts = parse_backup_timestamp("dodo/dodo-2026-02-12T030000Z.db.gz");
        assert!(ts.is_some());
        let dt = ts.unwrap();
        assert_eq!(dt.format("%Y-%m-%d %H:%M:%S").to_string(), "2026-02-12 03:00:00");
    }

    #[test]
    fn parse_timestamp_different_date() {
        let ts = parse_backup_timestamp("dodo/dodo-2025-01-01T120000Z.db.gz");
        assert!(ts.is_some());
        let dt = ts.unwrap();
        assert_eq!(dt.format("%Y-%m-%d %H:%M:%S").to_string(), "2025-01-01 12:00:00");
    }

    #[test]
    fn parse_timestamp_deep_path() {
        let ts = parse_backup_timestamp("a/b/c/dodo-2026-06-15T183000Z.db.gz");
        assert!(ts.is_some());
        let dt = ts.unwrap();
        assert_eq!(dt.format("%Y-%m-%d %H:%M").to_string(), "2026-06-15 18:30");
    }

    #[test]
    fn parse_timestamp_no_prefix_dir() {
        let ts = parse_backup_timestamp("dodo-2026-02-12T030000Z.db.gz");
        assert!(ts.is_some());
    }

    #[test]
    fn parse_timestamp_missing_dodo_prefix() {
        let ts = parse_backup_timestamp("backup-2026-02-12T030000Z.db.gz");
        assert!(ts.is_none());
    }

    #[test]
    fn parse_timestamp_wrong_suffix() {
        let ts = parse_backup_timestamp("dodo/dodo-2026-02-12T030000Z.db");
        assert!(ts.is_none());
    }

    #[test]
    fn parse_timestamp_malformed_date() {
        let ts = parse_backup_timestamp("dodo/dodo-not-a-date.db.gz");
        assert!(ts.is_none());
    }

    #[test]
    fn parse_timestamp_empty() {
        let ts = parse_backup_timestamp("");
        assert!(ts.is_none());
    }

    #[test]
    fn parse_timestamp_just_suffix() {
        let ts = parse_backup_timestamp(".db.gz");
        assert!(ts.is_none());
    }
}
