use anyhow::Result;
use chrono::NaiveDate;
use scraper::{Html, Selector};
use std::time::Duration;

use crate::models::SearchConfig;
use crate::scraper::ScrapedItem;

const BORM_BASE_URL: &str = "https://www.borm.es";
const DEFAULT_OBSCURA_PATH: &str = "/home/oc/bin/obscura";
const FETCH_TIMEOUT_SECS: u64 = 30;

/// Scraper para el BORM (Boletín Oficial de la Región de Murcia) usando Obscura CLI.
///
/// El BORM usa Radware Captcha que bloquea requests normales.
/// Requiere Obscura con --stealth para bypass.
///
/// # Estrategia:
/// 1. Llama `obscura fetch --stealth --dump html` para obtener el último boletín
/// 2. Llama el endpoint de sumario con la fecha del boletín
/// 3. Parsea el pseudo-XML devuelto dentro del HTML
/// 4. Filtra anuncios por keywords contra el campo `<sumario>`
pub async fn scrape(config: &SearchConfig) -> Result<Vec<ScrapedItem>> {
    let keywords: Vec<String> = config
        .keywords
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let obscura_path = std::env::var("OBSCURA_PATH").unwrap_or_else(|_| DEFAULT_OBSCURA_PATH.to_string());

    // Step 1: Fetch the latest bulletin metadata
    let ultimo_html = match obscura_fetch(&obscura_path, "https://www.borm.es/services/boletin/ultimo").await {
        Ok(html) => html,
        Err(e) => {
            tracing::warn!("BORM: Error fetching ultimo from Obscura: {}", e);
            return Ok(Vec::new());
        }
    };

    // Parse the latest bulletin info
    let (fecha, _bulletin_id) = match parse_ultimo(&ultimo_html) {
        Some(info) => info,
        None => {
            tracing::warn!("BORM: Could not parse ultimo response");
            return Ok(Vec::new());
        }
    };

    tracing::info!("BORM: Latest bulletin date: {}", fecha);

    // Step 2: Fetch the summary for the bulletin date
    let fecha_str = fecha.format("%d-%m-%Y").to_string();
    let sumario_url = format!(
        "https://www.borm.es/services/boletin/fecha/{}/sumario",
        fecha_str
    );

    let sumario_html = match obscura_fetch(&obscura_path, &sumario_url).await {
        Ok(html) => html,
        Err(e) => {
            tracing::warn!("BORM: Error fetching sumario from Obscura: {}", e);
            return Ok(Vec::new());
        }
    };

    // Step 3: Parse announcements from the summary
    let announcements = parse_sumario(&sumario_html, &keywords, &fecha);

    tracing::info!(
        "BORM Murcia: {} items encontrados (fecha: {})",
        announcements.len(),
        fecha
    );

    Ok(announcements)
}

/// Run obscura fetch with stealth mode and return the HTML output.
async fn obscura_fetch(obscura_path: &str, url: &str) -> Result<String> {
    let output = tokio::time::timeout(
        Duration::from_secs(FETCH_TIMEOUT_SECS),
        tokio::process::Command::new(obscura_path)
            .args(["fetch", "--stealth", "--dump", "html", "--quiet", url])
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Obscura fetch timed out after {}s for {}", FETCH_TIMEOUT_SECS, url))??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Obscura fetch failed for {}: {}", url, stderr.trim());
    }

    let html = String::from_utf8(output.stdout)
        .map_err(|e| anyhow::anyhow!("Obscura output not valid UTF-8 for {}: {}", url, e))?;

    Ok(html)
}

/// Parse the ultimo response to extract (fecha_publicacion, bulletin_id).
/// The HTML wraps pseudo-XML like: <boletindto><id>110496</id>...<fechapublicacion>02-05-2026</fechapublicacion>...</boletindto>
fn parse_ultimo(html: &str) -> Option<(NaiveDate, String)> {
    let document = Html::parse_document(html);

    let id_text = select_text(&document, "id");
    let fecha_text = select_text(&document, "fechapublicacion");

    let id = id_text?;
    let fecha_str = fecha_text?;

    let fecha = NaiveDate::parse_from_str(&fecha_str, "%d-%m-%Y").ok()?;

    Some((fecha, id))
}

/// Parse the sumario response to extract announcements, filtered by keywords.
fn parse_sumario(html: &str, keywords: &[String], fecha: &NaiveDate) -> Vec<ScrapedItem> {
    let document = Html::parse_document(html);
    let mut items = Vec::new();

    // Select all individual <anunciosboletin> elements (children of the outer <anunciosboletin>)
    let selector = match Selector::parse("anunciosboletin > anunciosboletin") {
        Ok(s) => s,
        Err(_) => return items,
    };

    for element in document.select(&selector) {
        let id = select_text_in(&element, "id");
        let sumario = select_text_in(&element, "sumario");
        let numero = select_text_in(&element, "numero");
        let ano = select_text_in(&element, "ano");
        let apartado = select_text_in(&element, "apartado");
        let subapartado = select_text_in(&element, "subapartado");

        // Skip if essential fields are missing
        let (Some(id), Some(numero), Some(ano)) = (id.as_deref(), numero.as_deref(), ano.as_deref()) else {
            continue;
        };

        let sumario_text = sumario.unwrap_or_default();

        // Filter by keywords (match against sumario text)
        if !keywords.is_empty() {
            let sumario_lower = sumario_text.to_lowercase();
            if !keywords.iter().any(|kw| sumario_lower.contains(kw)) {
                continue;
            }
        }

        // Build the announcement URL
        let url = format!(
            "{}/#/home/anuncio/{}/{}/{}",
            BORM_BASE_URL, id, numero, ano
        );

        // Truncate title to 200 chars
        let title = if sumario_text.len() > 200 {
            sumario_text[..200].to_string()
        } else {
            sumario_text
        };

        // Build description from apartado + subapartado
        let description = match (apartado.as_deref(), subapartado.as_deref()) {
            (Some(a), Some(s)) => Some(format!("{} — {}", a, s)),
            (Some(a), None) => Some(a.to_string()),
            (None, Some(s)) => Some(s.to_string()),
            (None, None) => None,
        };

        // Parse the publication date from the announcement, fallback to bulletin date
        let pub_date = element
            .select(&Selector::parse("fechapublicacion").unwrap())
            .next()
            .and_then(|el| el.text().collect::<String>().trim().to_string().into())
            .and_then(|s: String| NaiveDate::parse_from_str(&s, "%d-%m-%Y").ok())
            .unwrap_or(*fecha);

        items.push(ScrapedItem {
            title: Some(title),
            description,
            url: Some(url),
            external_id: Some(format!("{}/{}", ano, numero)),
            raw_data: None,
            published_at: Some(pub_date.and_hms_opt(0, 0, 0).unwrap()),
        });
    }

    items
}

/// Select the text content of the first element matching the tag name in the whole document.
fn select_text(document: &Html, tag: &str) -> Option<String> {
    let selector = Selector::parse(tag).ok()?;
    let element = document.select(&selector).next()?;
    let text = element.text().collect::<String>();
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Select the text content of the first child element matching the tag name within a given element.
fn select_text_in(element: &scraper::ElementRef, tag: &str) -> Option<String> {
    let selector = Selector::parse(tag).ok()?;
    let child = element.select(&selector).next()?;
    let text = child.text().collect::<String>();
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_borm_url_construction() {
        let url = format!(
            "{}/#/home/anuncio/{}/{}/{}",
            BORM_BASE_URL, "842595", "1913", "2026"
        );
        assert_eq!(url, "https://www.borm.es/#/home/anuncio/842595/1913/2026");
    }

    #[test]
    fn test_parse_ultimo() {
        let html = r#"<html><body><boletindto><id>110496</id><numero>99</numero><ano>2026</ano><fechapublicacion>02-05-2026</fechapublicacion></boletindto></body></html>"#;
        let result = parse_ultimo(html);
        assert!(result.is_some());
        let (fecha, id) = result.unwrap();
        assert_eq!(id, "110496");
        assert_eq!(fecha, NaiveDate::from_ymd_opt(2026, 5, 2).unwrap());
    }

    #[test]
    fn test_parse_sumario() {
        let html = r#"<html><body><sumarioboletindto><anunciosboletin><anunciosboletin>
            <id>842595</id>
            <sumario>Orden de 21 de abril de 2026 de contratacion</sumario>
            <numero>1913</numero>
            <ano>2026</ano>
            <anunciante>Consejeria de Economia</anunciante>
            <apartado>I. Comunidad Autonoma</apartado>
            <subapartado>2. Autoridades y Personal</subapartado>
            <fechapublicacion>02-05-2026</fechapublicacion>
        </anunciosboletin></anunciosboletin></sumarioboletindto></body></html>"#;

        let fecha = NaiveDate::from_ymd_opt(2026, 5, 2).unwrap();
        let items = parse_sumario(html, &[], &fecha);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].external_id.as_deref(), Some("2026/1913"));
        assert_eq!(
            items[0].url.as_deref(),
            Some("https://www.borm.es/#/home/anuncio/842595/1913/2026")
        );
        assert_eq!(
            items[0].description.as_deref(),
            Some("I. Comunidad Autonoma — 2. Autoridades y Personal")
        );
    }

    #[test]
    fn test_parse_sumario_with_keyword_filter() {
        let html = r#"<html><body><sumarioboletindto><anunciosboletin>
            <anunciosboletin><id>1</id><sumario>contratacion public</sumario><numero>10</numero><ano>2026</ano></anunciosboletin>
            <anunciosboletin><id>2</id><sumario>subvencion agricultura</sumario><numero>11</numero><ano>2026</ano></anunciosboletin>
        </anunciosboletin></sumarioboletindto></body></html>"#;

        let fecha = NaiveDate::from_ymd_opt(2026, 5, 2).unwrap();
        let keywords: Vec<String> = vec!["contratacion".to_string()];
        let items = parse_sumario(html, &keywords, &fecha);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].external_id.as_deref(), Some("2026/10"));
    }
}
