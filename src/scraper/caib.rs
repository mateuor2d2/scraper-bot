use anyhow::{Context, Result};
use reqwest::Client;
use scraper::{Html, Selector};

use crate::models::SearchConfig;
use crate::scraper::ScrapedItem;

// URLs conocidas del portal de contratacion de la CAIB
const CAIB_DEFAULT_URL: &str = "https://www.caib.es/sites/contratacion/ca/licitaciones/";
const CAIB_ALT_URL: &str = "https://contractaciopublica.caib.es/";

pub async fn scrape(config: &SearchConfig) -> Result<Vec<ScrapedItem>> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .user_agent("Mozilla/5.0 (compatible; ScraperBot/0.1)")
        .build()?;

    let url = if config.url.contains("caib.es") {
        &config.url
    } else {
        CAIB_DEFAULT_URL
    };

    let keywords: Vec<String> = config
        .keywords
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let resp = client.get(url).send().await?;
    let body = resp.text().await?;

    // Si el portal devuelve pagina de autenticacion/error, intentamos URL alternativa
    if body.contains("autenticaci") || body.contains("error") || body.contains("Mòdul d'autenticació") {
        tracing::warn!("Portal CAIB principal requiere autenticacion. Intentando alternativa...");
        let alt_resp = client.get(CAIB_ALT_URL).send().await;
        if let Ok(alt_resp) = alt_resp {
            let alt_body = alt_resp.text().await.unwrap_or_default();
            if !alt_body.is_empty() && alt_body.len() > 1000 {
                return parse_caib_html(&alt_body, &keywords, CAIB_ALT_URL).await;
            }
        }
    }

    parse_caib_html(&body, &keywords, url).await
}

async fn parse_caib_html(body: &str, keywords: &[String], base_url: &str) -> Result<Vec<ScrapedItem>> {
    let document = Html::parse_document(body);

    // Selectores heuristicos para tablas de licitaciones
    let row_selector = Selector::parse("tr, .fila, .licitacion, .expediente, article").unwrap();
    let link_selector = Selector::parse("a[href]").unwrap();

    let mut items = Vec::new();

    for row in document.select(&row_selector) {
        let text = row.text().collect::<Vec<_>>().join(" ").trim().to_string();
        if text.is_empty() {
            continue;
        }

        // Buscar link dentro de la fila
        let url = row
            .select(&link_selector)
            .next()
            .and_then(|a| a.value().attr("href"))
            .map(|href| resolve_url(base_url, href));

        let title = row
            .select(&Selector::parse("td:nth-child(1), .titulo, .title, h2, h3").unwrap())
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .unwrap_or_else(|| {
                // Si no hay titulo especifico, usar primeras palabras del texto
                text.split_whitespace().take(12).collect::<Vec<_>>().join(" ")
            });

        let description = if text.len() > title.len() {
            Some(text)
        } else {
            None
        };

        // Generar external_id a partir de la URL o del texto
        let external_id = url.as_ref().map(|u| {
            u.split('/').last().unwrap_or(u).to_string()
        });

        // Filtrar por keywords
        if !keywords.is_empty() {
            let search_text = format!(
                "{} {}",
                title.to_lowercase(),
                description.as_deref().unwrap_or("").to_lowercase()
            );
            if !keywords.iter().any(|kw| search_text.contains(kw)) {
                continue;
            }
        }

        items.push(ScrapedItem {
            title: if title.is_empty() { None } else { Some(title) },
            description,
            url,
            external_id,
            raw_data: None,
            published_at: None,
        });
    }

    // Si no encontramos filas, probar con selectores mas genericos (divs, listas)
    if items.is_empty() {
        let generic_selector = Selector::parse(".item, .resultado, li, .list-group-item").unwrap();
        for element in document.select(&generic_selector) {
            let text = element.text().collect::<Vec<_>>().join(" ").trim().to_string();
            if text.len() < 20 {
                continue;
            }

            let url = element
                .select(&link_selector)
                .next()
                .and_then(|a| a.value().attr("href"))
                .map(|href| resolve_url(base_url, href));

            let title = text.split('\n').next().unwrap_or(&text).trim().to_string();
        let description = if text.len() > title.len() {
            Some(text.clone())
        } else {
            None
        };

        // Filtrar por keywords
        if !keywords.is_empty() {
            let search_text = text.to_lowercase();
            if !keywords.iter().any(|kw| search_text.contains(kw)) {
                continue;
            }
        }

        items.push(ScrapedItem {
            title: if title.is_empty() { None } else { Some(title) },
            description,
            url: url.clone(),
            external_id: url.clone(),
            raw_data: None,
            published_at: None,
        });
        }
    }

    tracing::info!("Scraper CAIB: {} items encontrados", items.len());
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
