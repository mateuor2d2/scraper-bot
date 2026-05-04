use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::{Path, State},
    http::{request::Parts, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
    extract::FromRequestParts,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db::Db;
use crate::config::Config;
use crate::scraper;

use super::PublicApiState;
use super::auth::{AuthenticatedApiKey, ApiKeyState};
use super::rate_limit::RateLimiter;

// ============================================================
// Request / Response types
// ============================================================

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub company: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub api_key: String,
    pub plan: String,
    pub monthly_limit: i64,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub boletin: String,
    pub keywords: String,
    #[serde(default)]
    pub date_from: Option<String>,
    #[serde(default)]
    pub date_to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchCreatedResponse {
    pub id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct SearchStatusResponse {
    pub id: String,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchResultResponse {
    pub id: String,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub results: Option<serde_json::Value>,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UsageResponse {
    pub plan: String,
    pub month: String,
    pub request_count: i64,
    pub limit_count: i64,
    pub remaining: i64,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ============================================================
// Router builders
// ============================================================

/// Public routes that don't require authentication
pub fn register_router(state: PublicApiState) -> Router {
    Router::new()
        .route("/api/v1/register", post(register_handler))
        .route("/api/v1/health", get(health_handler))
        .with_state(state)
}

/// Protected routes that require Bearer token auth
pub fn protected_router(state: PublicApiState, auth_state: ApiKeyState) -> Router {
    Router::new()
        .route("/api/v1/search", post(search_handler))
        .route("/api/v1/search/{id}/status", get(search_status_handler))
        .route("/api/v1/search/{id}/result", get(search_result_handler))
        .route("/api/v1/usage", get(usage_handler))
        .with_state(state)
        .layer(axum::middleware::from_fn_with_state(
            auth_state,
            auth_middleware_fn,
        ))
}

// ============================================================
// Auth middleware (axum middleware function)
// ============================================================

async fn auth_middleware_fn(
    axum::extract::State(auth_state): axum::extract::State<ApiKeyState>,
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> impl IntoResponse {
    // Extract parts from the request
    let (mut parts, body) = req.into_parts();

    let auth_result = AuthenticatedApiKey::from_request_parts(&mut parts, &auth_state).await;

    match auth_result {
        Ok(api_key) => {
            // Reconstruct request with the authenticated key in extensions
            let mut req = axum::extract::Request::from_parts(parts, body);
            req.extensions_mut().insert(api_key);
            next.run(req).await
        }
        Err((status, body)) => (status, body).into_response(),
    }
}

// ============================================================
// Handlers
// ============================================================

async fn health_handler() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "version": "v1"
    }))
}

/// POST /api/v1/register — Register a new API user and get an API key.
async fn register_handler(
    State(state): State<PublicApiState>,
    Json(body): Json<RegisterRequest>,
) -> impl IntoResponse {
    // Validate input
    if body.email.is_empty() || !body.email.contains('@') {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Valid email is required".to_string(),
            }),
        )
            .into_response();
    }

    if body.company.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Company name is required".to_string(),
            }),
        )
            .into_response();
    }

    // Check if email already exists
    let existing: Option<String> =
        sqlx::query_scalar("SELECT id FROM api_keys WHERE email = ?")
            .bind(&body.email)
            .fetch_optional(&state.db.pool)
            .await
            .unwrap_or(None);

    if existing.is_some() {
        return (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Email already registered".to_string(),
            }),
        )
            .into_response();
    }

    // Generate API key: sk-scraper-{uuid}
    let key_id = uuid::Uuid::new_v4().to_string();
    let plain_key = format!("sk-scraper-{}", uuid::Uuid::new_v4());
    let key_hash = match super::auth::hash_api_key(&plain_key) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("Failed to hash API key: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to generate API key".to_string(),
                }),
            )
                .into_response();
        }
    };

    let plan = "free";
    let monthly_limit = RateLimiter::monthly_limit_for_plan(plan);

    // Insert into DB
    let result = sqlx::query(
        "INSERT INTO api_keys (id, key_hash, email, company, plan) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&key_id)
    .bind(&key_hash)
    .bind(&body.email)
    .bind(&body.company)
    .bind(plan)
    .execute(&state.db.pool)
    .await;

    if let Err(e) = result {
        tracing::error!("Failed to insert API key: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to register".to_string(),
            }),
        )
            .into_response();
    }

    // Initialize usage for current month
    let month = chrono::Local::now().format("%Y-%m").to_string();
    let usage_id = uuid::Uuid::new_v4().to_string();
    let _ = sqlx::query(
        "INSERT INTO api_usage (id, api_key_id, month, request_count, limit_count) VALUES (?, ?, ?, 0, ?)",
    )
    .bind(&usage_id)
    .bind(&key_id)
    .bind(&month)
    .bind(monthly_limit)
    .execute(&state.db.pool)
    .await;

    tracing::info!("New API key registered: email={} company={}", body.email, body.company);

    (
        StatusCode::CREATED,
        Json(RegisterResponse {
            api_key: plain_key,
            plan: plan.to_string(),
            monthly_limit,
            message: "IMPORTANT: Save this API key now. It will NOT be shown again.".to_string(),
        }),
    )
        .into_response()
}

/// POST /api/v1/search — Launch a search via the scraper engine.
async fn search_handler(
    State(state): State<PublicApiState>,
    axum::Extension(api_key): axum::Extension<AuthenticatedApiKey>,
    Json(body): Json<SearchRequest>,
) -> impl IntoResponse {
    // Check monthly usage limit
    let month = chrono::Local::now().format("%Y-%m").to_string();
    let monthly_limit = RateLimiter::monthly_limit_for_plan(&api_key.plan);

    let current_count: i64 = sqlx::query_scalar(
        "SELECT request_count FROM api_usage WHERE api_key_id = ? AND month = ?",
    )
    .bind(&api_key.id)
    .bind(&month)
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None)
    .unwrap_or(0);

    if current_count >= monthly_limit {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorResponse {
                error: format!(
                    "Monthly limit exceeded ({}/{}). Upgrade to pro for higher limits.",
                    current_count, monthly_limit
                ),
            }),
        )
            .into_response();
    }

    // Validate boletin type
    let valid_boletines = [
        "contratacion_estado",
        "generic_html",
        "boe_rss",
        "caib_licitaciones",
        "bocyl_rss",
        "doe_rss",
        "boc_canarias_rss",
        "borm_murcia",
    ];

    if !valid_boletines.contains(&body.boletin.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!(
                    "Invalid boletin type. Valid types: {:?}",
                    valid_boletines
                ),
            }),
        )
            .into_response();
    }

    // Create search record
    let search_id = uuid::Uuid::new_v4().to_string();

    let insert_result = sqlx::query(
        "INSERT INTO api_searches (id, api_key_id, boletin, keywords, date_from, date_to, status) VALUES (?, ?, ?, ?, ?, ?, 'pending')",
    )
    .bind(&search_id)
    .bind(&api_key.id)
    .bind(&body.boletin)
    .bind(&body.keywords)
    .bind(&body.date_from)
    .bind(&body.date_to)
    .execute(&state.db.pool)
    .await;

    if let Err(e) = insert_result {
        tracing::error!("Failed to create API search: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to create search".to_string(),
            }),
        )
            .into_response();
    }

    // Increment usage counter
    let usage_id = uuid::Uuid::new_v4().to_string();
    let _ = sqlx::query(
        "INSERT INTO api_usage (id, api_key_id, month, request_count, limit_count)
         VALUES (?, ?, ?, 1, ?)
         ON CONFLICT(api_key_id, month) DO UPDATE SET request_count = request_count + 1",
    )
    .bind(&usage_id)
    .bind(&api_key.id)
    .bind(&month)
    .bind(monthly_limit)
    .execute(&state.db.pool)
    .await;

    // Update status to running
    let _ = sqlx::query("UPDATE api_searches SET status = 'running' WHERE id = ?")
        .bind(&search_id)
        .execute(&state.db.pool)
        .await;

    // Run the scraper in a spawned task so we can return immediately
    let db_pool = state.db.pool.clone();
    let search_id_clone = search_id.clone();
    let search_type = body.boletin.clone();
    let keywords = body.keywords.clone();

    tokio::spawn(async move {
        let start = Instant::now();

        // Build a proper config for the scraper
        let config = crate::models::SearchConfig {
            id: 0,
            telegram_id: 0,
            name: format!("API: {}", search_type),
            url: search_type.clone(),
            search_type: search_type.clone(),
            keywords: if keywords.is_empty() { None } else { Some(keywords) },
            css_selector: None,
            notify_mode: "none".to_string(),
            filters: None,
            is_active: true,
            created_at: chrono::Local::now().naive_local(),
            updated_at: chrono::Local::now().naive_local(),
        };

        let result = scraper::run_scrape(&config).await;
        let elapsed = start.elapsed();

        match result {
            Ok(items) => {
                let result_json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string());
                let _ = sqlx::query(
                    "UPDATE api_searches SET status = 'completed', completed_at = CURRENT_TIMESTAMP, result_json = ? WHERE id = ?",
                )
                .bind(&result_json)
                .bind(&search_id_clone)
                .execute(&db_pool)
                .await;

                tracing::info!(
                    "API search {} completed: {} items in {:?}",
                    search_id_clone,
                    items.len(),
                    elapsed
                );
            }
            Err(e) => {
                let _ = sqlx::query(
                    "UPDATE api_searches SET status = 'failed', completed_at = CURRENT_TIMESTAMP, error_message = ? WHERE id = ?",
                )
                .bind(e.to_string())
                .bind(&search_id_clone)
                .execute(&db_pool)
                .await;

                tracing::error!(
                    "API search {} failed: {}",
                    search_id_clone,
                    e
                );
            }
        }
    });

    (
        StatusCode::ACCEPTED,
        Json(SearchCreatedResponse {
            id: search_id,
            status: "running".to_string(),
            message: "Search started. Poll /api/v1/search/{id}/status for updates.".to_string(),
        }),
    )
        .into_response()
}

/// GET /api/v1/search/:id/status — Check search status.
async fn search_status_handler(
    State(state): State<PublicApiState>,
    axum::Extension(api_key): axum::Extension<AuthenticatedApiKey>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let row: Option<ApiSearchRow> = sqlx::query_as::<_, ApiSearchRow>(
        "SELECT id, api_key_id, status, created_at, completed_at, error_message FROM api_searches WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None);

    match row {
        Some(row) => {
            // Ensure the authenticated user owns this search
            if row.api_key_id != api_key.id {
                return (
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse {
                        error: "You don't have access to this search".to_string(),
                    }),
                )
                    .into_response();
            }

            (
                StatusCode::OK,
                Json(SearchStatusResponse {
                    id: row.id,
                    status: row.status,
                    created_at: row.created_at.to_string(),
                    completed_at: row.completed_at.map(|d| d.to_string()),
                    error_message: row.error_message,
                }),
            )
                .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Search not found".to_string(),
            }),
        )
            .into_response(),
    }
}

/// GET /api/v1/search/:id/result — Get search results.
async fn search_result_handler(
    State(state): State<PublicApiState>,
    axum::Extension(api_key): axum::Extension<AuthenticatedApiKey>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let row: Option<ApiSearchFullRow> = sqlx::query_as::<_, ApiSearchFullRow>(
        "SELECT id, api_key_id, status, created_at, completed_at, result_json, error_message FROM api_searches WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None);

    match row {
        Some(row) => {
            if row.api_key_id != api_key.id {
                return (
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse {
                        error: "You don't have access to this search".to_string(),
                    }),
                )
                    .into_response();
            }

            let results: Option<serde_json::Value> = row
                .result_json
                .and_then(|json_str| serde_json::from_str(&json_str).ok());

            (
                StatusCode::OK,
                Json(SearchResultResponse {
                    id: row.id,
                    status: row.status,
                    created_at: row.created_at.to_string(),
                    completed_at: row.completed_at.map(|d| d.to_string()),
                    results,
                    error_message: row.error_message,
                }),
            )
                .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Search not found".to_string(),
            }),
        )
            .into_response(),
    }
}

/// GET /api/v1/usage — Get current monthly usage.
async fn usage_handler(
    State(state): State<PublicApiState>,
    axum::Extension(api_key): axum::Extension<AuthenticatedApiKey>,
) -> impl IntoResponse {
    let month = chrono::Local::now().format("%Y-%m").to_string();
    let monthly_limit = RateLimiter::monthly_limit_for_plan(&api_key.plan);

    let row: Option<ApiUsageRow> = sqlx::query_as::<_, ApiUsageRow>(
        "SELECT request_count, limit_count FROM api_usage WHERE api_key_id = ? AND month = ?",
    )
    .bind(&api_key.id)
    .bind(&month)
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None);

    let (request_count, limit_count) = match row {
        Some(r) => (r.request_count, r.limit_count),
        None => (0, monthly_limit),
    };

    let remaining = (limit_count - request_count).max(0);

    (
        StatusCode::OK,
        Json(UsageResponse {
            plan: api_key.plan,
            month,
            request_count,
            limit_count,
            remaining,
        }),
    )
        .into_response()
}

// ============================================================
// Internal DB row types
// ============================================================

#[derive(Debug, sqlx::FromRow)]
struct ApiSearchRow {
    id: String,
    api_key_id: String,
    status: String,
    created_at: chrono::NaiveDateTime,
    completed_at: Option<chrono::NaiveDateTime>,
    error_message: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct ApiSearchFullRow {
    id: String,
    api_key_id: String,
    status: String,
    created_at: chrono::NaiveDateTime,
    completed_at: Option<chrono::NaiveDateTime>,
    result_json: Option<String>,
    error_message: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct ApiUsageRow {
    request_count: i64,
    limit_count: i64,
}
