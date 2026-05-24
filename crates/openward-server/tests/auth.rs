use openward_db::{apply_schema, create_pool};
use sqlx::SqlitePool;

async fn setup() -> SqlitePool {
    let pool = create_pool(":memory:").await.expect("pool");
    apply_schema(&pool).await.expect("migrations");
    pool
}

// We can't call openward_server::auth directly (it's not pub), so inline the same logic here.
// This tests the argon2 crate integration with the same functions.

fn hash_password(password: &str) -> String {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    use argon2::Argon2;

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .expect("hash")
        .to_string()
}

fn verify_password(password: &str, hash: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    use argon2::Argon2;

    let parsed = PasswordHash::new(hash).expect("parse hash");
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// Insert an admin operator. Always inserts (no noop check). Returns the operator id.
async fn insert_admin(pool: &SqlitePool) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let pw_hash = hash_password("changeme");

    sqlx::query(
        "INSERT INTO operators (id, username, display_name, password_hash, role, language, is_active, created_at) \
         VALUES (?, 'admin', 'Administrateur', ?, 'admin', 'fr', 1, ?)"
    )
    .bind(&id)
    .bind(&pw_hash)
    .bind(&now)
    .execute(pool)
    .await
    .expect("insert admin");

    id
}

/// Mimics the server's seed_default_admin: no-op if any operators exist.
async fn seed_default_admin(pool: &SqlitePool) {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM operators")
        .fetch_one(pool)
        .await
        .expect("count");

    if count.0 > 0 {
        return;
    }

    insert_admin(pool).await;
}

async fn create_session(pool: &SqlitePool, operator_id: &str) -> String {
    let session_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let expires = now + chrono::Duration::hours(24);

    sqlx::query(
        "INSERT INTO sessions (id, operator_id, display_name, role, language, created_at, expires_at) \
         VALUES (?, ?, 'Administrateur', 'admin', 'fr', ?, ?)"
    )
    .bind(&session_id)
    .bind(operator_id)
    .bind(now.to_rfc3339())
    .bind(expires.to_rfc3339())
    .execute(pool)
    .await
    .expect("insert session");

    session_id
}

#[tokio::test]
async fn password_hash_verify_roundtrip() {
    let hash = hash_password("s3cret!");
    assert!(verify_password("s3cret!", &hash));
}

#[tokio::test]
async fn wrong_password_fails_verification() {
    let hash = hash_password("correct-password");
    assert!(!verify_password("wrong-password", &hash));
}

#[tokio::test]
async fn seed_default_admin_creates_account() {
    let pool = setup().await;
    insert_admin(&pool).await;

    let row: (String, String, String) = sqlx::query_as(
        "SELECT username, role, language FROM operators WHERE username = 'admin'"
    )
    .fetch_one(&pool)
    .await
    .expect("admin row");

    assert_eq!(row.0, "admin");
    assert_eq!(row.1, "admin");
    assert_eq!(row.2, "fr");

    // Verify the stored password hash works
    let pw_hash: (String,) = sqlx::query_as(
        "SELECT password_hash FROM operators WHERE username = 'admin'"
    )
    .fetch_one(&pool)
    .await
    .expect("pw hash");

    assert!(verify_password("changeme", &pw_hash.0));
}

#[tokio::test]
async fn seed_default_admin_is_noop_when_operators_exist() {
    let pool = setup().await;

    // Insert an existing operator
    sqlx::query(
        "INSERT INTO operators (id, username, display_name, password_hash, role, language, is_active, created_at) \
         VALUES ('existing-op', 'boss', 'The Boss', '$argon2id$placeholder', 'admin', 'fr', 1, '2025-01-01T00:00:00Z')"
    )
    .execute(&pool)
    .await
    .expect("insert existing");

    // Seed should be a no-op
    seed_default_admin(&pool).await;

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM operators")
        .fetch_one(&pool)
        .await
        .expect("count");

    assert_eq!(count.0, 1, "should still have exactly 1 operator");
}

#[tokio::test]
async fn session_creation_and_lookup() {
    let pool = setup().await;
    let admin_id = insert_admin(&pool).await;
    let session_id = create_session(&pool, &admin_id).await;

    // Session should be found
    let now = chrono::Utc::now().to_rfc3339();
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT operator_id, display_name, role FROM sessions WHERE id = ? AND expires_at > ?"
    )
    .bind(&session_id)
    .bind(&now)
    .fetch_optional(&pool)
    .await
    .expect("query");

    assert!(row.is_some());
    let (op_id, name, role) = row.unwrap();
    assert_eq!(op_id, admin_id);
    assert_eq!(name, "Administrateur");
    assert_eq!(role, "admin");
}

#[tokio::test]
async fn expired_session_not_found() {
    let pool = setup().await;
    let admin_id = insert_admin(&pool).await;

    // Create an expired session
    let session_id = uuid::Uuid::new_v4().to_string();
    let past = chrono::Utc::now() - chrono::Duration::hours(48);
    let expired = past + chrono::Duration::hours(24); // expired 24h ago

    sqlx::query(
        "INSERT INTO sessions (id, operator_id, display_name, role, language, created_at, expires_at) \
         VALUES (?, ?, 'Administrateur', 'admin', 'fr', ?, ?)"
    )
    .bind(&session_id)
    .bind(&admin_id)
    .bind(past.to_rfc3339())
    .bind(expired.to_rfc3339())
    .execute(&pool)
    .await
    .expect("insert expired session");

    let now = chrono::Utc::now().to_rfc3339();
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM sessions WHERE id = ? AND expires_at > ?"
    )
    .bind(&session_id)
    .bind(&now)
    .fetch_optional(&pool)
    .await
    .expect("query");

    assert!(row.is_none(), "expired session should not be returned");
}

#[tokio::test]
async fn logout_deletes_session() {
    let pool = setup().await;
    let admin_id = insert_admin(&pool).await;
    let session_id = create_session(&pool, &admin_id).await;

    // Delete the session (simulating logout)
    sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(&session_id)
        .execute(&pool)
        .await
        .expect("delete session");

    // Session should be gone
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM sessions WHERE id = ?"
    )
    .bind(&session_id)
    .fetch_optional(&pool)
    .await
    .expect("query");

    assert!(row.is_none(), "session should be deleted after logout");
}

#[tokio::test]
async fn password_change_invalidates_other_sessions() {
    let pool = setup().await;
    let admin_id = insert_admin(&pool).await;

    // Create two sessions
    let session1 = create_session(&pool, &admin_id).await;
    let session2 = create_session(&pool, &admin_id).await;

    // Simulate password change: delete all sessions except current (session1)
    sqlx::query("DELETE FROM sessions WHERE operator_id = ? AND id != ?")
        .bind(&admin_id)
        .bind(&session1)
        .execute(&pool)
        .await
        .expect("delete other sessions");

    // session1 should still exist
    let row1: Option<(String,)> = sqlx::query_as("SELECT id FROM sessions WHERE id = ?")
        .bind(&session1)
        .fetch_optional(&pool)
        .await
        .expect("query");
    assert!(row1.is_some(), "current session should survive password change");

    // session2 should be gone
    let row2: Option<(String,)> = sqlx::query_as("SELECT id FROM sessions WHERE id = ?")
        .bind(&session2)
        .fetch_optional(&pool)
        .await
        .expect("query");
    assert!(row2.is_none(), "other sessions should be invalidated on password change");
}
