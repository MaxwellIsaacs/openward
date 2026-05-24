use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

/// Create a backup of the database using VACUUM INTO.
/// Returns the path to the backup file.
pub async fn create_backup(
    pool: &SqlitePool,
    backup_dir: &str,
) -> Result<PathBuf, String> {
    let dir = Path::new(backup_dir);
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("failed to create backup dir: {}", e))?;

    let now = chrono::Utc::now();
    let filename = format!("openward-{}.db", now.format("%Y-%m-%d-%H%M%S"));
    let dest = dir.join(&filename);
    let dest_str = dest.to_string_lossy().to_string();

    sqlx::query(&format!("VACUUM INTO '{}'", dest_str.replace('\'', "''")))
        .execute(pool)
        .await
        .map_err(|e| format!("VACUUM INTO failed: {}", e))?;

    Ok(dest)
}

/// Remove the oldest backups beyond the retention count.
pub fn prune_backups(backup_dir: &str, keep: u32) -> Result<(), String> {
    let dir = Path::new(backup_dir);
    if !dir.exists() {
        return Ok(());
    }

    let mut backups: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("failed to read backup dir: {}", e))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("openward-") && n.ends_with(".db"))
                .unwrap_or(false)
        })
        .collect();

    // Sort ascending by filename (lexicographic == chronological for our format)
    backups.sort();

    if backups.len() > keep as usize {
        let to_remove = backups.len() - keep as usize;
        for path in backups.into_iter().take(to_remove) {
            let _ = std::fs::remove_file(path);
        }
    }

    Ok(())
}

/// Get the timestamp of the most recent backup from the filename.
pub fn last_backup_time(backup_dir: &str) -> Option<String> {
    let dir = Path::new(backup_dir);
    if !dir.exists() {
        return None;
    }

    let mut backups: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("openward-") && name.ends_with(".db") {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    backups.sort();
    backups.last().map(|name| {
        // openward-YYYY-MM-DD-HHMMSS.db -> YYYY-MM-DD HH:MM:SS
        let ts = name
            .strip_prefix("openward-")
            .unwrap_or(name)
            .strip_suffix(".db")
            .unwrap_or(name);
        // ts = "2026-03-20-143022"
        if ts.len() >= 17 {
            format!(
                "{} {}:{}:{}",
                &ts[..10],
                &ts[11..13],
                &ts[13..15],
                &ts[15..17]
            )
        } else {
            ts.to_string()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_create_backup() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
        let pool = sqlx::SqlitePool::connect(&db_url).await.unwrap();
        sqlx::query("CREATE TABLE t (x INTEGER)")
            .execute(&pool)
            .await
            .unwrap();

        let backup_dir = dir.path().join("backups");
        let result = create_backup(&pool, backup_dir.to_str().unwrap()).await;
        assert!(result.is_ok());
        let backup_path = result.unwrap();
        assert!(backup_path.exists());
        assert!(backup_path.metadata().unwrap().len() > 0);
    }

    #[test]
    fn test_prune_backups() {
        let dir = tempfile::tempdir().unwrap();
        let backup_dir = dir.path().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        for i in 1..=5 {
            let name = format!("openward-2026-03-{:02}-120000.db", i);
            fs::write(backup_dir.join(&name), b"test").unwrap();
        }

        prune_backups(backup_dir.to_str().unwrap(), 3).unwrap();

        let remaining: Vec<_> = fs::read_dir(&backup_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(remaining.len(), 3);
    }

    #[test]
    fn test_last_backup_time() {
        let dir = tempfile::tempdir().unwrap();
        let backup_dir = dir.path().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        assert_eq!(last_backup_time(backup_dir.to_str().unwrap()), None);

        fs::write(
            backup_dir.join("openward-2026-03-15-100000.db"),
            b"old",
        )
        .unwrap();
        fs::write(
            backup_dir.join("openward-2026-03-20-143022.db"),
            b"new",
        )
        .unwrap();

        let result = last_backup_time(backup_dir.to_str().unwrap());
        assert_eq!(result, Some("2026-03-20 14:30:22".to_string()));
    }
}
