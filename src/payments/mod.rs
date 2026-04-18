use stripe::{CheckoutSession, CheckoutSessionMode, Client, CreateCheckoutSession, CreateCheckoutSessionLineItems, CreateCheckoutSessionLineItemsPriceData, CreateCheckoutSessionLineItemsPriceDataProductData, Currency};
use std::sync::Arc;

use crate::config::StripeConfig;

pub struct StripeClient {
    client: Client,
    config: Arc<StripeConfig>,
}

impl StripeClient {
    pub fn new(config: Arc<StripeConfig>) -> Self {
        let client = Client::new(&config.secret_key);
        Self { client, config }
    }

    pub async fn create_subscription_checkout(
        &self,
        telegram_id: i64,
        searches_count: i32,
        monthly_price_eur: f64,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let success_url = format!("{}/payment/success?session_id={{CHECKOUT_SESSION_ID}}", self.config.base_url);
        let cancel_url = format!("{}/payment/cancel", self.config.base_url);

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("telegram_id".to_string(), telegram_id.to_string());
        metadata.insert("searches_count".to_string(), searches_count.to_string());

        let session = CheckoutSession::create(&self.client, CreateCheckoutSession {
            line_items: Some(vec![CreateCheckoutSessionLineItems {
                price_data: Some(CreateCheckoutSessionLineItemsPriceData {
                    currency: Currency::EUR,
                    product_data: Some(CreateCheckoutSessionLineItemsPriceDataProductData {
                        name: format!("Suscripción {} búsquedas", searches_count),
                        description: Some(format!("Acceso mensual a {} búsquedas configuradas", searches_count)),
                        ..Default::default()
                    }),
                    unit_amount: Some((monthly_price_eur * 100.0) as i64),
                    recurring: Some(stripe::CreateCheckoutSessionLineItemsPriceDataRecurring {
                        interval: stripe::CreateCheckoutSessionLineItemsPriceDataRecurringInterval::Month,
                        interval_count: Some(1),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                quantity: Some(1),
                ..Default::default()
            }]),
            mode: Some(CheckoutSessionMode::Subscription),
            success_url: Some(&success_url),
            cancel_url: Some(&cancel_url),
            client_reference_id: Some(&format!("{}_{}", telegram_id, searches_count)),
            metadata: Some(metadata),
            ..Default::default()
        }).await?;

        Ok(session.url.ok_or("No checkout URL returned")?.to_string())
    }
}
