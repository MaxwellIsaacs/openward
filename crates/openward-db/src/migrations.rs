use sqlx::SqlitePool;

/// Apply all pending database migrations.
///
/// Uses sqlx's built-in migration runner which tracks applied versions
/// in the `_sqlx_migrations` table. Safe to call on an already-migrated
/// database — only unapplied migrations will run.
pub async fn apply_schema(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
