use anyhow::Result;
use reqwest::Client;
use scraper::{Html, Selector};

use crate::models::SearchConfig;
use crate::scraper::ScrapedItem;

pub async fn scrape_html(config: &SearchConfig) -> Result<Vec<ScrapedItem>> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (compatible; ScraperBot/0.1)")
        .build()?;

    let resp = client.get(&config.url).send().await?;
    let body = resp.text().await?;
    let document = Html::parse_document(&body);

    let selector_str = config.css_selector.as_deref().unwrap_or("a");
    let selector = Selector::parse(selector_str).map_err(|e| anyhow::anyhow!("CSS selector invalid: {:?}", e))?;

    let mut items = Vec::new();
    for element in document.select(&selector) {
        let title = element.text().collect::<Vec<_>>().join(" ").trim().to_string();
        let url = element
            .value()
            .attr("href")
            .map(|u| resolve_url(&config.url, u));

        let mut description = None;
        // intentar hermano siguiente o elemento padre para descripción
        if let Some(parent) = element.parent().and_then(|p| p.value().as_element()) {
            // heurística simple: no hacemos nada complejo aquí
            let _ = parent;
        }

        items.push(ScrapedItem {
            title: if title.is_empty() { None } else { Some(title) },
            description,
            url,
            external_id: None,
            raw_data: None,
            published_at: None,
        });
    }

    // Filtrar por keywords si existen
    if let Some(ref keywords) = config.keywords {
        let kw_list: Vec<String> = keywords.split(',').map(|s| s.trim().to_lowercase()).collect();
        items.retain(|item| {
            let text = format!(
                "{} {}",
                item.title.as_deref().unwrap_or(""),
                item.description.as_deref().unwrap_or("")
            )
            .to_lowercase();
            kw_list.iter().any(|kw| text.contains(kw))
        });
    }

    Ok(items)
}

fn resolve_url(base: &str, href: &str) -> String {
    if href.starts_with("http") {
        href.to_string()
    } else {
        let base_url = reqwest::Url::parse(base).unwrap_or_else(|_| reqwest::Url::parse("https://example.com").unwrap());
        base_url.join(href).map(|u| u.to_string()).unwrap_or_else(|_| href.to_string())
    }
}
