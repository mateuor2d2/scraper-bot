use std::sync::Arc;

use axum::{
    extract::{FromRequestParts, State},
    http::request::Parts,
    http::StatusCode,
    response::IntoResponse,
};
use argon2::{Argon2, PasswordHash, PasswordVerifier, PasswordHasher};
use argon2::password_hash::SaltString;
use serde::Serialize;

use crate::db::Db;

/// State needed by the auth middleware
#[derive(Clone)]
pub struct ApiKeyState {
    pub db: Arc<Db>,
}

/// The authenticated API key info extracted from the Bearer token.
#[derive(Debug, Clone)]
pub struct AuthenticatedApiKey {
    pub id: String,
    pub plan: String,
}

/// Error response for auth failures
#[derive(Serialize)]
pub struct AuthError {
    pub error: String,
}

/// Extract the Bearer token from the Authorization header,
/// validate it against hashed keys in the DB.
#[axum::async_trait]
impl FromRequestParts<ApiKeyState> for AuthenticatedApiKey {
    type Rejection = (StatusCode, axum::Json<AuthError>);

    async fn from_request_parts(parts: &mut Parts, state: &ApiKeyState) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or((
                StatusCode::UNAUTHORIZED,
                axum::Json(AuthError {
                    error: "Missing Authorization header".to_string(),
                }),
            ))?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or((
                StatusCode::UNAUTHORIZED,
                axum::Json(AuthError {
                    error: "Invalid Authorization header format. Use: Bearer <api_key>".to_string(),
                }),
            ))?
            .trim();

        validate_api_key(state, token).await
    }
}

async fn validate_api_key(
    state: &ApiKeyState,
    plain_key: &str,
) -> Result<AuthenticatedApiKey, (StatusCode, axum::Json<AuthError>)> {
    // Keys are of the form sk-scraper-{uuid}
    if !plain_key.starts_with("sk-scraper-") {
        return Err((
            StatusCode::UNAUTHORIZED,
            axum::Json(AuthError {
                error: "Invalid API key format".to_string(),
            }),
        ));
    }

    // Look up all active keys and try to verify
    let rows = sqlx::query_as::<_, ApiKeyRow>(
        "SELECT id, key_hash, plan, active FROM api_keys WHERE active = 1",
    )
    .fetch_all(&state.db.pool)
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(AuthError {
                error: "Database error".to_string(),
            }),
        )
    })?;

    let argon2 = Argon2::default();

    for row in &rows {
        if let Ok(parsed_hash) = PasswordHash::new(&row.key_hash) {
            if argon2.verify_password(plain_key.as_bytes(), &parsed_hash).is_ok() {
                return Ok(AuthenticatedApiKey {
                    id: row.id.clone(),
                    plan: row.plan.clone(),
                });
            }
        }
    }

    Err((
        StatusCode::UNAUTHORIZED,
        axum::Json(AuthError {
            error: "Invalid API key".to_string(),
        }),
    ))
}

/// Hash a plain-text API key with argon2.
pub fn hash_api_key(plain_key: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut rand_core::OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(plain_key.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Failed to hash password: {}", e))?
        .to_string();
    Ok(hash)
}

// Internal row struct for DB query
#[derive(Debug, sqlx::FromRow)]
struct ApiKeyRow {
    id: String,
    key_hash: String,
    plan: String,
    active: bool,
}
