use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db::Db;
use crate::config::Config;

use super::AdminApiState;

#[derive(Deserialize)]
pub struct AdminQuery {
    pub token: String,
}

#[derive(Serialize)]
pub struct DashboardStats {
    pub total_users: i64,
    pub total_searches: i64,
    pub total_results_today: i64,
    pub active_subscriptions: i64,
    pub recent_scrapes: Vec<ScrapeLogResponse>,
}

#[derive(Serialize)]
pub struct ScrapeLogResponse {
    pub id: i64,
    pub search_config_id: i64,
    pub config_name: String,
    pub status: String,
    pub items_found: i32,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct UserResponse {
    pub telegram_id: i64,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub is_admin: bool,
    pub is_active: bool,
    pub created_at: String,
    pub search_count: i64,
}

#[derive(Serialize)]
pub struct SearchResponse {
    pub id: i64,
    pub telegram_id: i64,
    pub name: String,
    pub url: String,
    pub search_type: String,
    pub keywords: Option<String>,
    pub notify_mode: String,
    pub is_active: bool,
    pub created_at: String,
    pub result_count: i64,
}

fn check_admin_token(state: &AdminApiState, token: &str) -> bool {
    if let Ok(admin_token) = std::env::var("ADMIN_TOKEN") {
        if !admin_token.is_empty() && token == admin_token {
            return true;
        }
    }
    if let Ok(id) = token.parse::<i64>() {
        return state.config.bot.admins.contains(&id);
    }
    false
}

pub fn router(state: AdminApiState) -> Router {
    Router::new()
        .route("/api/admin/dashboard", get(dashboard))
        .route("/api/admin/users", get(list_users))
        .route("/api/admin/searches", get(list_searches))
        .route("/api/admin/results", get(list_results))
        .route("/api/admin/logs", get(list_logs))
        .route("/api/health", get(health))
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    Json(json!({"status": "ok"}))
}

async fn dashboard(
    State(state): State<AdminApiState>,
    Query(params): Query<AdminQuery>,
) -> impl IntoResponse {
    if !check_admin_token(&state, &params.token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "Unauthorized"}))).into_response();
    }

    let total_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db.pool)
        .await
        .unwrap_or(0);

    let total_searches: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM search_configs WHERE is_active = TRUE")
        .fetch_one(&state.db.pool)
        .await
        .unwrap_or(0);

    let today = chrono::Local::now().naive_local().date().to_string();
    let total_results_today: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM search_results WHERE DATE(scraped_at) = ?",
    )
    .bind(&today)
    .fetch_one(&state.db.pool)
    .await
    .unwrap_or(0);

    let active_subscriptions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM subscriptions WHERE status = 'active'",
    )
    .fetch_one(&state.db.pool)
    .await
    .unwrap_or(0);

    let recent_scrapes = match sqlx::query_as::<_, crate::db::ScrapeLog>(
        "SELECT * FROM scrape_logs ORDER BY created_at DESC LIMIT 20",
    )
    .fetch_all(&state.db.pool)
    .await
    {
        Ok(logs) => {
            let mut result = Vec::new();
            for log in logs {
                let config_name: String = sqlx::query_scalar(
                    "SELECT name FROM search_configs WHERE id = ?",
                )
                .bind(log.search_config_id)
                .fetch_one(&state.db.pool)
                .await
                .unwrap_or_else(|_| "Desconocida".to_string());

                result.push(ScrapeLogResponse {
                    id: log.id,
                    search_config_id: log.search_config_id,
                    config_name,
                    status: log.status,
                    items_found: log.items_found,
                    error_message: log.error_message,
                    duration_ms: log.duration_ms,
                    created_at: log.created_at.to_string(),
                });
            }
            result
        }
        Err(_) => Vec::new(),
    };

    let stats = DashboardStats {
        total_users,
        total_searches,
        total_results_today,
        active_subscriptions,
        recent_scrapes,
    };

    (StatusCode::OK, Json(stats)).into_response()
}

async fn list_users(
    State(state): State<AdminApiState>,
    Query(params): Query<AdminQuery>,
) -> impl IntoResponse {
    if !check_admin_token(&state, &params.token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "Unauthorized"}))).into_response();
    }

    let users = match sqlx::query_as::<_, crate::models::User>(
        "SELECT * FROM users ORDER BY created_at DESC LIMIT 100",
    )
    .fetch_all(&state.db.pool)
    .await
    {
        Ok(users) => users,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response();
        }
    };

    let mut result = Vec::new();
    for user in users {
        let search_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM search_configs WHERE telegram_id = ?")
                .bind(user.telegram_id)
                .fetch_one(&state.db.pool)
                .await
                .unwrap_or(0);

        result.push(UserResponse {
            telegram_id: user.telegram_id,
            username: user.username,
            first_name: user.first_name,
            is_admin: user.is_admin,
            is_active: user.is_active,
            created_at: user.created_at.to_string(),
            search_count,
        });
    }

    (StatusCode::OK, Json(result)).into_response()
}

async fn list_searches(
    State(state): State<AdminApiState>,
    Query(params): Query<AdminQuery>,
) -> impl IntoResponse {
    if !check_admin_token(&state, &params.token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "Unauthorized"}))).into_response();
    }

    let searches = match sqlx::query_as::<_, crate::models::SearchConfig>(
        "SELECT * FROM search_configs ORDER BY created_at DESC LIMIT 200",
    )
    .fetch_all(&state.db.pool)
    .await
    {
        Ok(searches) => searches,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response();
        }
    };

    let mut result = Vec::new();
    for search in searches {
        let result_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM search_results WHERE search_config_id = ?")
                .bind(search.id)
                .fetch_one(&state.db.pool)
                .await
                .unwrap_or(0);

        result.push(SearchResponse {
            id: search.id,
            telegram_id: search.telegram_id,
            name: search.name,
            url: search.url,
            search_type: search.search_type,
            keywords: search.keywords,
            notify_mode: search.notify_mode,
            is_active: search.is_active,
            created_at: search.created_at.to_string(),
            result_count,
        });
    }

    (StatusCode::OK, Json(result)).into_response()
}

#[derive(Serialize)]
pub struct ResultResponse {
    pub id: i64,
    pub search_config_id: i64,
    pub config_name: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub external_id: Option<String>,
    pub scraped_at: String,
    pub notified: bool,
}

async fn list_results(
    State(state): State<AdminApiState>,
    Query(params): Query<AdminQuery>,
) -> impl IntoResponse {
    if !check_admin_token(&state, &params.token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "Unauthorized"}))).into_response();
    }

    let results = match sqlx::query_as::<_, crate::models::SearchResultWithConfig>(
        "SELECT r.id, r.search_config_id, r.title, r.description, r.url, r.external_id, r.raw_data, r.published_at, r.scraped_at, r.notified, c.name as config_name
         FROM search_results r
         JOIN search_configs c ON r.search_config_id = c.id
         ORDER BY r.scraped_at DESC LIMIT 100",
    )
    .fetch_all(&state.db.pool)
    .await
    {
        Ok(results) => results,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response();
        }
    };

    let response: Vec<ResultResponse> = results
        .into_iter()
        .map(|r| ResultResponse {
            id: r.id,
            search_config_id: r.search_config_id,
            config_name: r.config_name,
            title: r.title,
            url: r.url,
            external_id: r.external_id,
            scraped_at: r.scraped_at.to_string(),
            notified: r.notified,
        })
        .collect();

    (StatusCode::OK, Json(response)).into_response()
}

async fn list_logs(
    State(state): State<AdminApiState>,
    Query(params): Query<AdminQuery>,
) -> impl IntoResponse {
    if !check_admin_token(&state, &params.token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "Unauthorized"}))).into_response();
    }

    let logs = match sqlx::query_as::<_, crate::db::ScrapeLog>(
        "SELECT * FROM scrape_logs ORDER BY created_at DESC LIMIT 100",
    )
    .fetch_all(&state.db.pool)
    .await
    {
        Ok(logs) => logs,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response();
        }
    };

    let mut result = Vec::new();
    for log in logs {
        let config_name: String = sqlx::query_scalar("SELECT name FROM search_configs WHERE id = ?")
            .bind(log.search_config_id)
            .fetch_one(&state.db.pool)
            .await
            .unwrap_or_else(|_| "Desconocida".to_string());

        result.push(ScrapeLogResponse {
            id: log.id,
            search_config_id: log.search_config_id,
            config_name,
            status: log.status,
            items_found: log.items_found,
            error_message: log.error_message,
            duration_ms: log.duration_ms,
            created_at: log.created_at.to_string(),
        });
    }

    (StatusCode::OK, Json(result)).into_response()
}
