use anyhow::{Context, Result};
use reqwest::Client;
use scraper::{Html, Selector};

use crate::models::SearchConfig;
use crate::scraper::ScrapedItem;

const BASE_URL: &str = "https://contrataciondelestado.es/wps/portal/plataforma/buscadores/detalle/!ut/p/z1/04_Sj9CPykssy0xPLMnMz0vMAfIjo8ziTVz9nZ3dPIwMLIKNXQyMfFxCQ808gFx3U_1wsAJTY2eTMK-wALNgT3cDA08PNxefUENTA3cjM_0oYvQb4ACOBsTpx6MgCr_x4fpR-K0wgirA50VClhTkhoZGGGR6AgA3hHJw/dz/d5/L2dJQSEvUUt3QS80TmxFL1o2XzRFT0NDRkgyMDhTM0QwMkxEVVU2SEgyMDgy/";

pub async fn scrape(config: &SearchConfig) -> Result<Vec<ScrapedItem>> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .user_agent("Mozilla/5.0 (compatible; ScraperBot/0.1)")
        .build()?;

    let keywords = config
        .keywords
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<String>>();

    let keyword = keywords.first().cloned().unwrap_or_default();

    // Nota: contrataciondelestado.es usa JSF (JavaServer Faces) con ViewState dinámico.
    // Un scraper fiable requiere browser automation (Chrome headless + WebDriver/CDP).
    // Este es un stub que hace un GET básico y parsea lo que pueda.
    let resp = client
        .get(BASE_URL)
        .query(&[("texto", keyword.as_str()), ("numpag", "1")])
        .send()
        .await?;

    let body = resp.text().await?;
    let document = Html::parse_document(&body);

    // Selectores heurísticos
    let row_selector = Selector::parse("tr").unwrap();
    let link_selector = Selector::parse("a[onclick]").unwrap();
    let re = regex::Regex::new(r"idLicitacion','(\d+)'")?;

    let mut items = Vec::new();

    for row in document.select(&row_selector) {
        let cells: Vec<String> = row
            .select(&Selector::parse("td").unwrap())
            .map(|td| td.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if cells.len() < 2 {
            continue;
        }

        let title = cells.get(0).cloned();
        let description = cells.get(1).cloned().or_else(|| cells.get(2).cloned());

        let onclick = row
            .select(&link_selector)
            .next()
            .and_then(|a| a.value().attr("onclick"));

        let external_id = onclick.and_then(|o| {
            re.captures(o)
                .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
        });

        let url = external_id.as_ref().map(|id| {
            format!(
                "https://contrataciondelestado.es/es/otra-seccion/detalle-del-proceso/idLicitacion/{}/",
                id
            )
        });

        items.push(ScrapedItem {
            title,
            description,
            url,
            external_id,
            raw_data: Some(serde_json::to_string(&cells).unwrap_or_default()),
            published_at: None,
        });
    }

    if items.is_empty() {
        tracing::warn!(
            "Scraper de contrataciondelestado.es devolvió 0 resultados. \
             Este portal requiere browser automation (Chrome headless) por su arquitectura JSF."
        );
    }

    Ok(items)
}
