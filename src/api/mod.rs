pub mod auth;
pub mod handlers;
pub mod rate_limit;

use std::sync::Arc;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};

use crate::db::Db;
use crate::config::Config;

use self::auth::ApiKeyState;
use self::rate_limit::RateLimiter;

/// Shared state for the public API (v1)
#[derive(Clone)]
pub struct PublicApiState {
    pub db: Arc<Db>,
    pub config: Arc<Config>,
    pub rate_limiter: Arc<RateLimiter>,
}

/// Admin API state (existing, kept for backwards compat)
#[derive(Clone)]
pub struct AdminApiState {
    pub db: Arc<Db>,
    pub config: Arc<Config>,
}

// ---- Admin API (existing routes, migrated from old src/api.rs) ----

pub mod admin;

pub fn admin_router(state: AdminApiState) -> Router {
    Router::new()
        .merge(admin::router(state))
}

// ---- Public API (v1) ----

pub fn public_router(state: PublicApiState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let auth_state = ApiKeyState {
        db: Arc::clone(&state.db),
    };

    Router::new()
        // Public: register
        .merge(handlers::register_router(state.clone()))
        // Protected: requires API key
        .merge(handlers::protected_router(state, auth_state))
        .layer(cors)
}
