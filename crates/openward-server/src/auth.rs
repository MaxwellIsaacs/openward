/// Actions that can be gated by role-based access control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Action {
    ViewDashboard,
    ViewPopulation,
    ViewDetaineeDetail,
    AdmitDetainee,
    UpdateBasisStatusHousing,
    ReleaseDetainee,
    ManageHousing,
    ManageOperators,
    ViewAuditTrail,
    AddNote,
    DeleteNote,
    ManageBackups,
    ExportData,
}

/// Check whether the given role is allowed to perform the given action.
pub fn can_do(role: &str, action: Action) -> bool {
    match action {
        Action::ViewDashboard | Action::ViewPopulation | Action::ViewDetaineeDetail => true,
        Action::AdmitDetainee | Action::UpdateBasisStatusHousing | Action::AddNote => role != "readonly",
        Action::ReleaseDetainee | Action::ManageHousing | Action::ViewAuditTrail | Action::DeleteNote => {
            role == "admin" || role == "supervisor"
        }
        Action::ManageOperators | Action::ManageBackups => role == "admin",
        Action::ExportData => role == "admin" || role == "supervisor",
    }
}

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use sqlx::SqlitePool;

/// Hash a password with argon2id and a random salt.
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// Verify a password against a stored argon2id hash.
pub fn verify_password(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed = PasswordHash::new(hash)?;
    match Argon2::default().verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Seed a default admin operator if no operators exist.
///
/// Inserts username `admin` with password `changeme`, role `admin`, language `fr`.
/// No-op if any operator already exists.
pub async fn seed_default_admin(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM operators")
        .fetch_one(pool)
        .await?;

    if count.0 > 0 {
        return Ok(());
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let password_hash = hash_password("changeme")
        .expect("failed to hash default admin password");

    sqlx::query(
        "INSERT INTO operators (id, username, display_name, password_hash, role, language, is_active, must_change_password, created_at) \
         VALUES (?, 'admin', 'Administrateur', ?, 'admin', 'fr', 1, 1, ?)"
    )
    .bind(&id)
    .bind(&password_hash)
    .bind(&now)
    .execute(pool)
    .await?;

    tracing::warn!("========================================");
    tracing::warn!("  DEFAULT ADMIN ACCOUNT CREATED");
    tracing::warn!("  Username: admin");
    tracing::warn!("  Password: changeme");
    tracing::warn!("  CHANGE THIS PASSWORD ON FIRST LOGIN!");
    tracing::warn!("========================================");
    Ok(())
}
