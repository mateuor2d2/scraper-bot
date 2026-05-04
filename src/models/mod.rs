use chrono::NaiveDateTime;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub telegram_id: i64,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub is_admin: bool,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SearchConfig {
    pub id: i64,
    pub telegram_id: i64,
    pub name: String,
    pub url: String,
    pub search_type: String,
    pub keywords: Option<String>,
    pub css_selector: Option<String>,
    pub notify_mode: String,
    pub filters: Option<String>,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SearchResult {
    pub id: i64,
    pub search_config_id: i64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub external_id: Option<String>,
    pub raw_data: Option<String>,
    pub published_at: Option<chrono::NaiveDateTime>,
    pub scraped_at: NaiveDateTime,
    pub notified: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SearchResultWithConfig {
    pub id: i64,
    pub search_config_id: i64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub external_id: Option<String>,
    pub raw_data: Option<String>,
    pub published_at: Option<chrono::NaiveDateTime>,
    pub scraped_at: NaiveDateTime,
    pub notified: bool,
    pub config_name: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Pricing {
    pub id: i64,
    pub price_per_search_eur: f64,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Subscription {
    pub id: i64,
    pub telegram_id: i64,
    pub active_searches: i32,
    pub monthly_price_eur: f64,
    pub paid_until: Option<chrono::NaiveDate>,
    pub stripe_customer_id: Option<String>,
    pub stripe_subscription_id: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UrlProfile {
    pub id: i64,
    pub domain: String,
    pub title_selector: Option<String>,
    pub item_selector: Option<String>,
    pub link_selector: Option<String>,
    pub description_selector: Option<String>,
    pub sample_url: Option<String>,
    pub confidence: i32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
