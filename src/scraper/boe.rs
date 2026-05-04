use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use reqwest::Client;
use rss::Channel;

use crate::filters::FilterConfig;
use crate::models::SearchConfig;
use crate::scraper::ScrapedItem;

// Default RSS URL for BOE job offers/oposiciones
// canal_per.php?l=p&c=140 covers the last 2 months of oposiciones
// Alternative: boe.php?s=2B for daily Section II.B (Oposiciones y concursos)
const BOE_RSS_URL: &str = "https://www.boe.es/rss/canal_per.php?l=p&c=140";

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

    // Parsear filtros avanzados si existen
    let filter_config = config.filters.as_deref()
        .map(FilterConfig::parse)
        .unwrap_or_default();

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
            if !keywords.iter().any(|kw| keyword_matches(&text, kw)) {
                continue;
            }
        }

        // Aplicar filtros avanzados (include/exclude)
        let full_text = format!(
            "{} {} {}",
            title,
            description.as_deref().unwrap_or(""),
            link.as_deref().unwrap_or("")
        );
        if !filter_config.matches(&full_text) {
            tracing::debug!("BOE item filtrado por filters: {}", title);
            continue;
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

/// Check if a keyword matches the text.
/// Supports two matching modes:
/// 1. Exact phrase match: "ingeniero industrial" matches "ingeniero industrial" (case-insensitive)
/// 2. Word-level AND match: "ingeniero industrial" matches if BOTH words appear anywhere in text
///    (e.g., matches "Cuerpo de Ingenieros Industriales" because "ingeniero" matches "ingenieros" 
///    as substring AND "industrial" matches "industriales")
/// 
/// Note: For plural forms, use truncated stems like "ingenier" which will match
/// "ingeniero", "ingenieros", "ingeniería", etc.
fn keyword_matches(text: &str, keyword: &str) -> bool {
    // First, try exact phrase match
    if text.contains(keyword) {
        return true;
    }
    
    // If keyword has multiple words, try AND matching on each word
    let words: Vec<&str> = keyword.split_whitespace().filter(|w| !w.is_empty()).collect();
    if words.len() > 1 {
        // Check if ALL words appear as substrings in the text
        return words.iter().all(|word| text.contains(word));
    }
    
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_matches_exact() {
        let text = "resolucion de ingenieros industriales del estado";
        assert!(keyword_matches(text, "ingenieros industriales"));
        assert!(keyword_matches(text, "resolucion"));
    }

    #[test]
    fn test_keyword_matches_word_level() {
        let text = "cuerpo de ingenieros industriales del estado";
        // "ingeniero industrial" should match because:
        // - "ingeniero" is in "ingenieros" (substring)
        // - "industrial" is in "industriales" (substring)
        assert!(keyword_matches(text, "ingeniero industrial"));
    }

    #[test]
    fn test_keyword_matches_partial_fail() {
        let text = "cuerpo de ingenieros del estado";
        // Should fail because "industrial" is not in text
        assert!(!keyword_matches(text, "ingeniero industrial"));
    }

    #[test]
    fn test_keyword_matches_with_stem() {
        let text = "cuerpo de ingenieros industriales del estado";
        // Using truncated stem "ingenier" matches all variants
        assert!(keyword_matches(text, "ingenier"));
        assert!(keyword_matches(text, "industrial"));
    }
}
