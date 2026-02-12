use sqlx::SqlitePool;

/// The full schema SQL, embedded at compile time.
const SCHEMA: &str = include_str!("schema.sql");

/// Apply the database schema. Uses CREATE TABLE IF NOT EXISTS, so it's
/// safe to call on an already-initialized database.
pub async fn apply_schema(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Split the schema into individual statements and execute each.
    // SQLite doesn't support multiple statements in a single execute call
    // through sqlx, so we split on semicolons.
    for statement in SCHEMA.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        sqlx::query(trimmed).execute(pool).await?;
    }
    Ok(())
}
