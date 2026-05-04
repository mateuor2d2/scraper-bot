use std::sync::Arc;
use axum::{
    body::Bytes,
    extract::State,
    http::{StatusCode, HeaderMap},
    response::IntoResponse,
    routing::post,
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use serde::Deserialize;

use crate::db::Db;
use crate::config::Config;
use crate::api::rate_limit::RateLimiter;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub config: Arc<Config>,
}

#[derive(Debug, Deserialize)]
pub struct StripeWebhookPayload {
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: StripeEventData,
}

#[derive(Debug, Deserialize)]
pub struct StripeEventData {
    pub object: serde_json::Value,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/webhook/stripe", post(handle_stripe_webhook))
        .with_state(state)
}

async fn handle_stripe_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let sig = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let secret = &state.config.stripe.webhook_secret;

    // Verificar firma del webhook si tenemos secreto configurado
    if !secret.is_empty() && !secret.starts_with("whsec_...") {
        let payload_str = match std::str::from_utf8(&body) {
            Ok(s) => s,
            Err(_) => return StatusCode::BAD_REQUEST,
        };

        if let Err(e) = stripe::Webhook::construct_event(payload_str, sig, secret) {
            tracing::warn!("Firma de webhook inválida: {}", e);
            return StatusCode::BAD_REQUEST;
        }
    }

    let payload: StripeWebhookPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("JSON de webhook inválido: {}", e);
            return StatusCode::BAD_REQUEST;
        }
    };

    tracing::info!("Webhook Stripe recibido: {}", payload.event_type);

    match payload.event_type.as_str() {
        "checkout.session.completed" => {
            let obj = &payload.data.object;

            let session_id = obj
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let metadata = obj.get("metadata").cloned().unwrap_or_default();
            let telegram_id = metadata
                .get("telegram_id")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok());

            let amount = obj
                .get("amount_total")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as f64
                / 100.0;

            let searches_count = metadata
                .get("searches_count")
                .and_then(|v| v.as_str())
                .unwrap_or("0")
                .parse::<i32>()
                .unwrap_or(0);

            let customer_id = obj
                .get("customer")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let subscription_id = obj
                .get("subscription")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let payment_intent_id = obj
                .get("payment_intent")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if let Some(telegram_id) = telegram_id {
                // Guardar registro del pago
                if let Err(e) = state
                    .db
                    .record_payment(
                        telegram_id,
                        &session_id,
                        payment_intent_id.as_deref(),
                        amount,
                        searches_count,
                        "succeeded",
                    )
                    .await
                {
                    tracing::error!("Error guardando pago: {}", e);
                }

                let paid_until = chrono::Local::now()
                    .naive_local()
                    .date()
                    .checked_add_months(chrono::Months::new(1));

                if let Err(e) = state
                    .db
                    .upsert_subscription_with_stripe(
                        telegram_id,
                        searches_count,
                        amount,
                        paid_until,
                        "active",
                        customer_id.as_deref(),
                        subscription_id.as_deref(),
                    )
                    .await
                {
                    tracing::error!("Error activando suscripción: {}", e);
                    return StatusCode::INTERNAL_SERVER_ERROR;
                }

                tracing::info!(
                    "Suscripción activada para {} - {} búsquedas - {:.2}€",
                    telegram_id,
                    searches_count,
                    amount
                );
            } else {
                tracing::warn!("Webhook sin telegram_id en metadata");
            }
        }
        "invoice.payment_failed" => {
            if let Some(customer) = payload.data.object.get("customer").and_then(|v| v.as_str()) {
                tracing::warn!("Pago fallido para cliente Stripe: {}", customer);
                if let Err(e) = state.db.set_subscription_status_by_customer(customer, "past_due").await {
                    tracing::error!("Error actualizando estado de suscripción: {}", e);
                }
            }
        }
        "customer.subscription.deleted" => {
            if let Some(sub) = payload.data.object.get("id").and_then(|v| v.as_str()) {
                tracing::info!("Suscripción cancelada en Stripe: {}", sub);
                if let Err(e) = state.db.set_subscription_status_by_stripe_sub(sub, "cancelled").await {
                    tracing::error!("Error cancelando suscripción: {}", e);
                }
            }
        }
        _ => {}
    }

    StatusCode::OK
}

/// Start the combined HTTP server: webhook + admin API + public API.
pub async fn start_webhook_server(db: Arc<Db>, config: Arc<Config>, port: u16) -> anyhow::Result<()> {
    let rate_limiter = Arc::new(RateLimiter::new());

    // Start rate limiter cleanup task
    let cleanup_limiter = Arc::clone(&rate_limiter);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
            cleanup_limiter.cleanup().await;
        }
    });

    // Webhook state
    let webhook_state = AppState {
        db: Arc::clone(&db),
        config: Arc::clone(&config),
    };

    // Admin API state
    let admin_state = crate::api::AdminApiState {
        db: Arc::clone(&db),
        config: Arc::clone(&config),
    };

    // Public API state
    let public_state = crate::api::PublicApiState {
        db: Arc::clone(&db),
        config: Arc::clone(&config),
        rate_limiter,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let static_dir = std::env::var("ADMIN_STATIC_DIR").unwrap_or_else(|_| "admin/.output/public".to_string());
    let index_path = format!("{}/index.html", static_dir);

    let app = router(webhook_state)
        .merge(crate::api::admin_router(admin_state))
        .merge(crate::api::public_router(public_state))
        .fallback_service(
            ServeDir::new(&static_dir)
                .fallback(ServeFile::new(&index_path))
        )
        .layer(cors);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("Servidor webhook/API escuchando en puerto {}", port);
    tracing::info!("Admin panel servido desde: {}", static_dir);
    tracing::info!("Public API v1 available at /api/v1/");
    axum::serve(listener, app).await?;
    Ok(())
}
