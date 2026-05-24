use openward_db::{apply_schema, create_pool};
use sqlx::Row;

async fn setup() -> sqlx::SqlitePool {
    let pool = create_pool(":memory:").await.expect("pool");
    apply_schema(&pool).await.expect("migrations");
    pool
}

#[tokio::test]
async fn fresh_database_gets_all_migrations() {
    let pool = setup().await;

    let rows = sqlx::query("SELECT version, description FROM _sqlx_migrations ORDER BY version")
        .fetch_all(&pool)
        .await
        .expect("query _sqlx_migrations");

    assert_eq!(rows.len(), 5, "expected 5 migrations applied");

    let desc0 = rows[0].get::<String, _>("description");
    let desc1 = rows[1].get::<String, _>("description");
    let desc2 = rows[2].get::<String, _>("description");
    let desc3 = rows[3].get::<String, _>("description");
    let desc4 = rows[4].get::<String, _>("description");
    assert!(
        desc0.contains("initial"),
        "first migration should be the initial schema, got: {desc0}"
    );
    assert!(
        desc1.contains("operators"),
        "second migration should add operators, got: {desc1}"
    );
    assert!(
        desc2.contains("sessions"),
        "third migration should add sessions, got: {desc2}"
    );
    assert!(
        desc3.contains("note") && desc3.contains("type"),
        "fourth migration should add note type, got: {desc3}"
    );
    assert!(
        desc4.contains("must") && desc4.contains("change") && desc4.contains("password"),
        "fifth migration should add must_change_password, got: {desc4}"
    );
}

#[tokio::test]
async fn migrations_are_idempotent() {
    let pool = setup().await;

    // Running migrations a second time should succeed without error.
    apply_schema(&pool).await.expect("second migration run");

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(count, 5, "still exactly 5 migrations after second run");
}

#[tokio::test]
async fn operators_table_exists_after_migration() {
    let pool = setup().await;

    sqlx::query(
        "INSERT INTO operators (id, username, display_name, password_hash, role, language, is_active, created_at)
         VALUES ('op-1', 'admin', 'Admin User', '$argon2id$hash', 'admin', 'fr', 1, '2025-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("insert operator");

    let username: String =
        sqlx::query_scalar("SELECT username FROM operators WHERE id = 'op-1'")
            .fetch_one(&pool)
            .await
            .expect("select operator");
    assert_eq!(username, "admin");
}
