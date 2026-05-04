use anyhow::Result;
use chrono::NaiveDateTime;
use reqwest::Client;
use rss::Channel;

use crate::models::SearchConfig;
use crate::scraper::ScrapedItem;

// Disposiciones Generales — most relevant for procurement
const BOC_RSS_URL: &str = "https://www.gobiernodecanarias.org/boc/feeds/capitulo/disposiciones_generales.rss";

pub async fn scrape(config: &SearchConfig) -> Result<Vec<ScrapedItem>> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (compatible; ScraperBot/0.1)")
        .build()?;

    let url = if config.url.contains("gobiernodecanarias.org/boc") {
        &config.url
    } else {
        BOC_RSS_URL
    };

    let resp = client.get(url).send().await?;
    let bytes = resp.bytes().await?;
    let channel = Channel::read_from(&bytes[..])
        .map_err(|e| anyhow::anyhow!("Error parseando RSS del BOC Canarias: {}", e))?;

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
        let pub_date = item.pub_date().and_then(|d| {
            chrono::DateTime::parse_from_rfc2822(d)
                .ok()
                .map(|dt| dt.naive_local())
        });

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

        let external_id = guid.clone().or_else(|| link.clone());

        let raw_data = serde_json::json!({
            "title": item.title(),
            "link": item.link(),
            "description": item.description(),
            "guid": item.guid().map(|g| g.value()),
            "pub_date": item.pub_date(),
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

    tracing::info!("Scraper BOC Canarias: {} items encontrados", items.len());
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boc_canarias_rss_parse() {
        let rss = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss xmlns:atom="http://www.w3.org/2005/Atom" version="2.0">
          <channel>
            <title>RSS I. Disposiciones generales</title>
            <item>
              <title>Resolución de la Consejería de Obras Públicas</title>
              <link>https://www.gobiernodecanarias.org/boc/anuncio/2026/12345</link>
              <description>Licitación de servicios de prevención de riesgos</description>
              <pubDate>Fri, 02 May 2026 00:00:00 +0200</pubDate>
            </item>
          </channel>
        </rss>"#;

        let channel = rss::Channel::read_from(rss.as_bytes()).unwrap();
        assert_eq!(channel.items().len(), 1);
        assert!(channel.items()[0].description().unwrap().contains("prevención"));
    }
}
