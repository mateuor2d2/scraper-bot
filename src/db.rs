use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::sync::Arc;
use std::time::Duration;

use crate::models::{Pricing, SearchConfig, SearchResult, Subscription, User};

#[derive(Debug, Clone)]
pub struct Db {
    pub pool: Pool<Sqlite>,
    pub query_timeout: Duration,
}

impl Db {
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&format!("sqlite:{}?mode=rwc", db_path))
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self {
            pool,
            query_timeout: Duration::from_secs(5),
        })
    }

    // ===== USERS =====
    pub async fn get_or_create_user(
        &self,
        telegram_id: i64,
        username: Option<&str>,
        first_name: Option<&str>,
        last_name: Option<&str>,
    ) -> anyhow::Result<User> {
        if let Ok(user) = sqlx::query_as::<_, User>("SELECT * FROM users WHERE telegram_id = ?")
            .bind(telegram_id)
            .fetch_one(&self.pool)
            .await
        {
            sqlx::query(
                "UPDATE users SET username = ?, first_name = ?, last_name = ?, updated_at = CURRENT_TIMESTAMP WHERE telegram_id = ?",
            )
            .bind(username)
            .bind(first_name)
            .bind(last_name)
            .bind(telegram_id)
            .execute(&self.pool)
            .await?;
            return Ok(user);
        }
        let user = sqlx::query_as::<_, User>(
            "INSERT INTO users (telegram_id, username, first_name, last_name) VALUES (?, ?, ?, ?) RETURNING *",
        )
        .bind(telegram_id)
        .bind(username)
        .bind(first_name)
        .bind(last_name)
        .fetch_one(&self.pool)
        .await?;
        Ok(user)
    }

    pub async fn get_user(&self, telegram_id: i64) -> anyhow::Result<Option<User>> {
        Ok(sqlx::query_as::<_, User>("SELECT * FROM users WHERE telegram_id = ?")
            .bind(telegram_id)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn set_user_admin(&self, telegram_id: i64, is_admin: bool) -> anyhow::Result<bool> {
        let res = sqlx::query("UPDATE users SET is_admin = ? WHERE telegram_id = ?")
            .bind(is_admin)
            .bind(telegram_id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn get_all_users(&self) -> anyhow::Result<Vec<User>> {
        Ok(sqlx::query_as::<_, User>("SELECT * FROM users ORDER BY created_at DESC LIMIT 100")
            .fetch_all(&self.pool)
            .await?)
    }

    // ===== PRICING =====
    pub async fn get_pricing(&self) -> anyhow::Result<Pricing> {
        let pricing = sqlx::query_as::<_, Pricing>("SELECT * FROM pricing WHERE id = 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(pricing)
    }

    pub async fn set_pricing(&self, price: f64) -> anyhow::Result<()> {
        sqlx::query("UPDATE pricing SET price_per_search_eur = ?, updated_at = CURRENT_TIMESTAMP WHERE id = 1")
            .bind(price)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ===== SEARCH CONFIGS =====
    pub async fn create_search_config(
        &self,
        telegram_id: i64,
        name: &str,
        url: &str,
        search_type: &str,
        keywords: Option<&str>,
        css_selector: Option<&str>,
        notify_mode: Option<&str>,
        filters: Option<&str>,
    ) -> anyhow::Result<i64> {
        let mode = notify_mode.unwrap_or("daily");
        let res = sqlx::query(
            "INSERT INTO search_configs (telegram_id, name, url, search_type, keywords, css_selector, notify_mode, filters) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(telegram_id)
        .bind(name)
        .bind(url)
        .bind(search_type)
        .bind(keywords)
        .bind(css_selector)
        .bind(mode)
        .bind(filters)
        .execute(&self.pool)
        .await?;
        Ok(res.last_insert_rowid())
    }

    pub async fn get_user_search_configs(&self, telegram_id: i64) -> anyhow::Result<Vec<SearchConfig>> {
        Ok(sqlx::query_as::<_, SearchConfig>(
            "SELECT * FROM search_configs WHERE telegram_id = ? AND is_active = TRUE ORDER BY created_at DESC",
        )
        .bind(telegram_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_active_search_configs(&self) -> anyhow::Result<Vec<SearchConfig>> {
        Ok(sqlx::query_as::<_, SearchConfig>(
            "SELECT * FROM search_configs WHERE is_active = TRUE ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_search_config(&self, id: i64) -> anyhow::Result<Option<SearchConfig>> {
        Ok(sqlx::query_as::<_, SearchConfig>("SELECT * FROM search_configs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn delete_search_config(&self, id: i64, telegram_id: i64) -> anyhow::Result<bool> {
        let res = sqlx::query("DELETE FROM search_configs WHERE id = ? AND telegram_id = ?")
            .bind(id)
            .bind(telegram_id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn update_search_config(
        &self,
        id: i64,
        telegram_id: i64,
        name: Option<&str>,
        url: Option<&str>,
        search_type: Option<&str>,
        keywords: Option<&str>,
        css_selector: Option<&str>,
        notify_mode: Option<&str>,
        filters: Option<&str>,
    ) -> anyhow::Result<bool> {
        let mut query = String::from("UPDATE search_configs SET updated_at = CURRENT_TIMESTAMP");
        let mut has_updates = false;
        
        if name.is_some() { query.push_str(", name = ?"); has_updates = true; }
        if url.is_some() { query.push_str(", url = ?"); has_updates = true; }
        if search_type.is_some() { query.push_str(", search_type = ?"); has_updates = true; }
        if keywords.is_some() { query.push_str(", keywords = ?"); has_updates = true; }
        if css_selector.is_some() { query.push_str(", css_selector = ?"); has_updates = true; }
        if notify_mode.is_some() { query.push_str(", notify_mode = ?"); has_updates = true; }
        if filters.is_some() { query.push_str(", filters = ?"); has_updates = true; }
        
        if !has_updates {
            return Ok(false);
        }
        
        query.push_str(" WHERE id = ? AND telegram_id = ?");
        
        let mut q = sqlx::query(&query);
        if let Some(v) = name { q = q.bind(v); }
        if let Some(v) = url { q = q.bind(v); }
        if let Some(v) = search_type { q = q.bind(v); }
        if let Some(v) = keywords { q = q.bind(v); }
        if let Some(v) = css_selector { q = q.bind(v); }
        if let Some(v) = notify_mode { q = q.bind(v); }
        if let Some(v) = filters { q = q.bind(v); }
        
        let res = q.bind(id).bind(telegram_id).execute(&self.pool).await?;
        Ok(res.rows_affected() > 0)
    }

    // ===== SEARCH RESULTS =====
    pub async fn save_search_result(
        &self,
        search_config_id: i64,
        title: Option<&str>,
        description: Option<&str>,
        url: Option<&str>,
        external_id: Option<&str>,
        raw_data: Option<&str>,
        published_at: Option<chrono::NaiveDateTime>,
    ) -> anyhow::Result<i64> {
        let res = sqlx::query(
            "INSERT INTO search_results (search_config_id, title, description, url, external_id, raw_data, published_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(search_config_id)
        .bind(title)
        .bind(description)
        .bind(url)
        .bind(external_id)
        .bind(raw_data)
        .bind(published_at)
        .execute(&self.pool)
        .await?;
        Ok(res.last_insert_rowid())
    }

    pub async fn result_exists(&self, search_config_id: i64, external_id: &str) -> anyhow::Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM search_results WHERE search_config_id = ? AND external_id = ?",
        )
        .bind(search_config_id)
        .bind(external_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    pub async fn get_unnotified_results(&self, telegram_id: i64) -> anyhow::Result<Vec<crate::models::SearchResultWithConfig>> {
        let rows = sqlx::query_as::<_, crate::models::SearchResultWithConfig>(
            "SELECT r.id, r.search_config_id, r.title, r.description, r.url, r.external_id, r.raw_data, r.published_at, r.scraped_at, r.notified, c.name as config_name
             FROM search_results r
             JOIN search_configs c ON r.search_config_id = c.id
             WHERE c.telegram_id = ? AND r.notified = FALSE
             ORDER BY r.scraped_at DESC",
        )
        .bind(telegram_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn mark_results_notified(&self, ids: Vec<i64>) -> anyhow::Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("UPDATE search_results SET notified = TRUE WHERE id IN ({})", placeholders);
        let mut query = sqlx::query(&sql);
        for id in ids {
            query = query.bind(id);
        }
        query.execute(&self.pool).await?;
        Ok(())
    }

    // ===== SUBSCRIPTIONS =====
    pub async fn get_subscription(&self, telegram_id: i64) -> anyhow::Result<Option<Subscription>> {
        Ok(sqlx::query_as::<_, Subscription>("SELECT * FROM subscriptions WHERE telegram_id = ?")
            .bind(telegram_id)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn upsert_subscription(
        &self,
        telegram_id: i64,
        active_searches: i32,
        monthly_price_eur: f64,
        paid_until: Option<chrono::NaiveDate>,
        status: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO subscriptions (telegram_id, active_searches, monthly_price_eur, paid_until, status)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(telegram_id) DO UPDATE SET
             active_searches = excluded.active_searches,
             monthly_price_eur = excluded.monthly_price_eur,
             paid_until = excluded.paid_until,
             status = excluded.status,
             updated_at = CURRENT_TIMESTAMP",
        )
        .bind(telegram_id)
        .bind(active_searches)
        .bind(monthly_price_eur)
        .bind(paid_until)
        .bind(status)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ===== PAYMENTS =====
    pub async fn record_payment(
        &self,
        telegram_id: i64,
        stripe_session_id: &str,
        stripe_payment_intent_id: Option<&str>,
        amount_eur: f64,
        searches_count: i32,
        status: &str,
    ) -> anyhow::Result<i64> {
        let res = sqlx::query(
            "INSERT INTO payments (telegram_id, stripe_session_id, stripe_payment_intent_id, amount_eur, searches_count, status, paid_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(stripe_session_id) DO UPDATE SET
             status = excluded.status,
             paid_at = excluded.paid_at,
             stripe_payment_intent_id = excluded.stripe_payment_intent_id",
        )
        .bind(telegram_id)
        .bind(stripe_session_id)
        .bind(stripe_payment_intent_id)
        .bind(amount_eur)
        .bind(searches_count)
        .bind(status)
        .bind(if status == "succeeded" { Some(chrono::Local::now().naive_local()) } else { None })
        .execute(&self.pool)
        .await?;
        Ok(res.last_insert_rowid())
    }

    pub async fn upsert_subscription_with_stripe(
        &self,
        telegram_id: i64,
        active_searches: i32,
        monthly_price_eur: f64,
        paid_until: Option<chrono::NaiveDate>,
        status: &str,
        stripe_customer_id: Option<&str>,
        stripe_subscription_id: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO subscriptions (telegram_id, active_searches, monthly_price_eur, paid_until, status, stripe_customer_id, stripe_subscription_id)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(telegram_id) DO UPDATE SET
             active_searches = excluded.active_searches,
             monthly_price_eur = excluded.monthly_price_eur,
             paid_until = excluded.paid_until,
             status = excluded.status,
             stripe_customer_id = COALESCE(excluded.stripe_customer_id, subscriptions.stripe_customer_id),
             stripe_subscription_id = COALESCE(excluded.stripe_subscription_id, subscriptions.stripe_subscription_id),
             updated_at = CURRENT_TIMESTAMP",
        )
        .bind(telegram_id)
        .bind(active_searches)
        .bind(monthly_price_eur)
        .bind(paid_until)
        .bind(status)
        .bind(stripe_customer_id)
        .bind(stripe_subscription_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_subscription_status_by_customer(
        &self,
        stripe_customer_id: &str,
        status: &str,
    ) -> anyhow::Result<bool> {
        let res = sqlx::query(
            "UPDATE subscriptions SET status = ?, updated_at = CURRENT_TIMESTAMP WHERE stripe_customer_id = ?",
        )
        .bind(status)
        .bind(stripe_customer_id)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn set_subscription_status_by_stripe_sub(
        &self,
        stripe_subscription_id: &str,
        status: &str,
    ) -> anyhow::Result<bool> {
        let res = sqlx::query(
            "UPDATE subscriptions SET status = ?, updated_at = CURRENT_TIMESTAMP WHERE stripe_subscription_id = ?",
        )
        .bind(status)
        .bind(stripe_subscription_id)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn get_url_profile(&self, domain: &str) -> anyhow::Result<Option<crate::models::UrlProfile>> {
        Ok(sqlx::query_as::<_, crate::models::UrlProfile>("SELECT * FROM url_profiles WHERE domain = ?")
            .bind(domain)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn save_url_profile(
        &self,
        domain: &str,
        title_selector: Option<&str>,
        item_selector: Option<&str>,
        link_selector: Option<&str>,
        description_selector: Option<&str>,
        sample_url: Option<&str>,
        confidence: i32,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO url_profiles (domain, title_selector, item_selector, link_selector, description_selector, sample_url, confidence)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(domain) DO UPDATE SET
             title_selector = excluded.title_selector,
             item_selector = excluded.item_selector,
             link_selector = excluded.link_selector,
             description_selector = excluded.description_selector,
             sample_url = excluded.sample_url,
             confidence = excluded.confidence,
             updated_at = CURRENT_TIMESTAMP",
        )
        .bind(domain)
        .bind(title_selector)
        .bind(item_selector)
        .bind(link_selector)
        .bind(description_selector)
        .bind(sample_url)
        .bind(confidence)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ===== DAILY REPORTS =====
    pub async fn record_daily_report(&self, telegram_id: i64, report_date: chrono::NaiveDate, new_results_count: i32) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO daily_reports (telegram_id, report_date, new_results_count) VALUES (?, ?, ?)",
        )
        .bind(telegram_id)
        .bind(report_date)
        .bind(new_results_count)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ===== SCRAPE LOGS =====
    pub async fn log_scrape(
        &self,
        search_config_id: i64,
        status: &str,
        items_found: i32,
        error_message: Option<&str>,
        duration_ms: i64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO scrape_logs (search_config_id, status, items_found, error_message, duration_ms) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(search_config_id)
        .bind(status)
        .bind(items_found)
        .bind(error_message)
        .bind(duration_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_recent_scrape_logs(&self, search_config_id: i64, limit: i64) -> anyhow::Result<Vec<ScrapeLog>> {
        let rows = sqlx::query_as::<_, ScrapeLog>(
            "SELECT * FROM scrape_logs WHERE search_config_id = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(search_config_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    // ===== NOTIFY MODE =====
    pub async fn set_notify_mode(&self, search_config_id: i64, telegram_id: i64, mode: &str) -> anyhow::Result<bool> {
        let res = sqlx::query(
            "UPDATE search_configs SET notify_mode = ? WHERE id = ? AND telegram_id = ?",
        )
        .bind(mode)
        .bind(search_config_id)
        .bind(telegram_id)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn get_unnotified_results_by_config(&self, search_config_id: i64) -> anyhow::Result<Vec<crate::models::SearchResultWithConfig>> {
        let rows = sqlx::query_as::<_, crate::models::SearchResultWithConfig>(
            "SELECT r.id, r.search_config_id, r.title, r.description, r.url, r.external_id, r.raw_data, r.published_at, r.scraped_at, r.notified, c.name as config_name
             FROM search_results r
             JOIN search_configs c ON r.search_config_id = c.id
             WHERE r.search_config_id = ? AND r.notified = FALSE
             ORDER BY r.scraped_at DESC",
        )
        .bind(search_config_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ScrapeLog {
    pub id: i64,
    pub search_config_id: i64,
    pub status: String,
    pub items_found: i32,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
    pub created_at: chrono::NaiveDateTime,
}
