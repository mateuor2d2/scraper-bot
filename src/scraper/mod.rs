use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapedItem {
    pub title: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub external_id: Option<String>,
    pub raw_data: Option<String>,
    pub published_at: Option<chrono::NaiveDateTime>,
}

pub mod boe;
pub mod caib;
pub mod contratacion_estado;
pub mod generic;

use crate::models::SearchConfig;

pub async fn run_scrape(config: &SearchConfig) -> Result<Vec<ScrapedItem>> {
    match config.search_type.as_str() {
        "contratacion_estado" => contratacion_estado::scrape(config).await,
        "generic_html" => generic::scrape_html(config).await,
        "boe_rss" => boe::scrape(config).await,
        "caib_licitaciones" => caib::scrape(config).await,
        _ => generic::scrape_html(config).await,
    }
}
