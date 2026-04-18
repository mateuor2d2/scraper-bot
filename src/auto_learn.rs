use anyhow::Result;
use reqwest::Client;
use scraper::{Html, Selector};
use std::time::Duration;
use url::Url;

use crate::db::Db;

#[derive(Debug, Clone, Default)]
pub struct LearnedProfile {
    pub domain: String,
    pub item_selector: Option<String>,
    pub title_selector: Option<String>,
    pub link_selector: Option<String>,
    pub description_selector: Option<String>,
    pub confidence: i32,
}

pub async fn learn_profile(db: &Db, url_str: &str) -> Result<LearnedProfile> {
    let parsed = Url::parse(url_str)?;
    let domain = parsed.host_str().unwrap_or(url_str).to_string();

    // Reutilizar perfil previo si existe y tiene confianza alta
    if let Some(existing) = db.get_url_profile(&domain).await? {
        if existing.confidence >= 70 {
            return Ok(LearnedProfile {
                domain: existing.domain,
                item_selector: existing.item_selector,
                title_selector: existing.title_selector,
                link_selector: existing.link_selector,
                description_selector: existing.description_selector,
                confidence: existing.confidence,
            });
        }
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("Mozilla/5.0 (compatible; ScraperBot/0.1)")
        .build()?;

    let body = client.get(url_str).send().await?.text().await?;

    let mut best = {
        let document = Html::parse_document(&body);

        let candidates = [
            "article",
            ".post",
            ".item",
            ".entry",
            ".news-item",
            ".list-item",
            ".card",
            "li.article",
            "div[class*=\"item\"]",
            "div[class*=\"post\"]",
            "div[class*=\"news\"]",
            "div[class*=\"card\"]",
        ];

        let mut best: Option<LearnedProfile> = None;
        let mut best_score = 0i32;

        for sel in &candidates {
            let Ok(selector) = Selector::parse(sel) else { continue };
            let elements: Vec<_> = document.select(&selector).collect();
            if elements.len() < 2 {
                continue;
            }

            let mut total_links = 0usize;
            let mut total_text_len = 0usize;
            let mut has_titles = 0usize;

            for el in &elements {
                let text = el.text().collect::<String>();
                total_text_len += text.trim().len();

                let links = el.select(&Selector::parse("a").unwrap()).count();
                total_links += links;

                if el.select(&Selector::parse("h1,h2,h3,h4,.title,[class*=\"title\"]").unwrap()).next().is_some() {
                    has_titles += 1;
                }
            }

            let avg_text = (total_text_len / elements.len()) as i32;
            let avg_links = (total_links / elements.len()) as i32;
            let title_ratio = (has_titles * 100 / elements.len()) as i32;

            let mut score = (elements.len() as i32).min(20) * 2;
            score += avg_text.min(200) / 10;
            score += avg_links * 5;
            score += title_ratio / 2;

            if score > best_score {
                best_score = score;
                best = Some(LearnedProfile {
                    domain: domain.clone(),
                    item_selector: Some(sel.to_string()),
                    title_selector: Some("h1,h2,h3,h4,.title,[class*=\"title\"]".to_string()),
                    link_selector: Some("a".to_string()),
                    description_selector: Some("p,.summary,.excerpt,[class*=\"desc\"]".to_string()),
                    confidence: score.min(100),
                });
            }
        }

        best
    };

    if let Some(ref mut profile) = best {
        db.save_url_profile(
            &profile.domain,
            profile.title_selector.as_deref(),
            profile.item_selector.as_deref(),
            profile.link_selector.as_deref(),
            profile.description_selector.as_deref(),
            Some(url_str),
            profile.confidence,
        )
        .await?;
    }

    Ok(best.unwrap_or_else(|| LearnedProfile {
        domain,
        confidence: 0,
        ..Default::default()
    }))
}
