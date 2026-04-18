use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use reqwest::Client;
use rss::Channel;

use crate::models::SearchConfig;
use crate::scraper::ScrapedItem;

const BOE_RSS_URL: &str = "https://www.boe.es/rss/boe.php?s=1";

pub async fn scrape(config: &SearchConfig) -> Result<Vec<ScrapedItem>> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (compatible; ScraperBot/0.1)")
        .build()?;

    // Si el usuario configuro una URL especifica del BOE, la usamos; si no, el sumario general
    let url = if config.url.contains("boe.es") {
        &config.url
    } else {
        BOE_RSS_URL
    };

    let resp = client.get(url).send().await?;
    let bytes = resp.bytes().await?;
    let channel = Channel::read_from(&bytes[..])
        .map_err(|e| anyhow::anyhow!("Error parseando RSS del BOE: {}", e))?;

    let keywords: Vec<String> = config
        .keywords
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let mut items = Vec::new();

    for item in channel.items() {
        let title = item.title().unwrap_or("").to_string();
        let description = item.description().map(|s| s.to_string());
        let link = item.link().map(|s| s.to_string());
        let guid = item.guid().map(|g| g.value().to_string());
        let pub_date = item.pub_date().and_then(|d| parse_rfc2822(d));

        // Filtrar por keywords si estan definidas
        if !keywords.is_empty() {
            let text = format!(
                "{} {} {}",
                title.to_lowercase(),
                description.as_deref().unwrap_or("").to_lowercase(),
                link.as_deref().unwrap_or("").to_lowercase()
            );
            if !keywords.iter().any(|kw| text.contains(kw)) {
                continue;
            }
        }

        // Generar ID externo a partir del GUID o del link
        let external_id = guid.clone().or_else(|| link.clone());

        let raw_data = serde_json::json!({
            "title": item.title(),
            "link": item.link(),
            "description": item.description(),
            "guid": item.guid().map(|g| g.value()),
            "pub_date": item.pub_date(),
            "categories": item.categories().iter().map(|c| c.name()).collect::<Vec<_>>(),
        });

        items.push(ScrapedItem {
            title: if title.is_empty() { None } else { Some(title) },
            description,
            url: link,
            external_id,
            raw_data: Some(raw_data.to_string()),
            published_at: pub_date,
        });
    }

    tracing::info!("Scraper BOE: {} items encontrados", items.len());
    Ok(items)
}

fn parse_rfc2822(date_str: &str) -> Option<NaiveDateTime> {
    chrono::DateTime::parse_from_rfc2822(date_str)
        .ok()
        .map(|dt| dt.naive_local())
}
