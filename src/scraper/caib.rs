use anyhow::{Context, Result};
use reqwest::Client;
use scraper::{Html, Selector};

use crate::models::SearchConfig;
use crate::scraper::ScrapedItem;

// URLs del portal de contratación de la CAIB
const CAIB_PORTAL_URL: &str = "https://www.caib.es/sites/contractaciopublica/ca/";
const CAIB_EBOIB_SEARCH_URL: &str = "https://www.caib.es/eboibfront/cercar";
const CAIB_EBOIB_RSS_URL: &str = "https://www.caib.es/eboibfront/indexrss.do?lang=ca";
const CAIB_EBOIB_BASE: &str = "https://www.caib.es";

/// Scrapea licitaciones del portal de contratación de la CAIB
/// 
/// Estrategia:
/// 1. Intenta usar el buscador del eboibfront (BOIB) con tipusContingut=123 (anunci licitació)
/// 2. Si falla, intenta el RSS del BOIB
/// 3. Extrae BOIBs que contienen licitaciones
/// 
/// Nota: La plataforma de contratación directa (plataformadecontractacio.caib.es) 
/// requiere acceso especial. Usamos el BOIB como fuente alternativa.
pub async fn scrape(config: &SearchConfig) -> Result<Vec<ScrapedItem>> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .user_agent("Mozilla/5.0 (compatible; ScraperBot/0.1)")
        .build()?;

    let keywords: Vec<String> = config
        .keywords
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    tracing::debug!("CAIB: Iniciando scraping con keywords: {:?}", keywords);

    // Estrategia 1: Buscar en eboibfront anuncios de licitación
    let items = scrape_eboib_licitaciones(&client, &keywords).await;
    
    if let Ok(items) = &items {
        if !items.is_empty() {
            tracing::info!("CAIB: Encontrados {} items via eboibfront search", items.len());
            return Ok(items.clone());
        }
    }
    
    tracing::warn!("CAIB: eboibfront search falló o vacío, intentando RSS");
    
    // Estrategia 2: Intentar RSS
    if let Ok(items) = scrape_eboib_rss(&client, &keywords).await {
        if !items.is_empty() {
            tracing::info!("CAIB: Encontrados {} items via RSS", items.len());
            return Ok(items);
        }
    }
    
    // Estrategia 3: Intentar portal principal (usualmente requiere auth, pero probamos)
    tracing::warn!("CAIB: RSS vacío, intentando portal principal");
    scrape_portal_principal(&client, &keywords).await
}

/// Scrapea licitaciones del buscador del eboibfront
/// Usa POST a /eboibfront/cercar con tipusContingut=123 (anunci licitació)
async fn scrape_eboib_licitaciones(client: &Client, keywords: &[String]) -> Result<Vec<ScrapedItem>> {
    tracing::debug!("CAIB: POST a {} con tipusContingut=123", CAIB_EBOIB_SEARCH_URL);
    
    let form_data = [
        ("tipusContingut", "123"), // 123 = anunci licitació
        ("cercar", "Cercar"),
    ];
    
    let resp = client
        .post(CAIB_EBOIB_SEARCH_URL)
        .form(&form_data)
        .send()
        .await
        .context("Failed to POST to eboibfront search")?;
    
    let body = resp.text().await.context("Failed to read eboibfront response")?;
    tracing::debug!("CAIB: eboibfront devolvió {} bytes", body.len());
    
    parse_eboib_search_html(&body, keywords)
}

/// Parsea el HTML de resultados de búsqueda del eboibfront
/// 
/// Estructura real detectada:
/// ```html
/// <li>
///   <div class="caja">
///     <div class="interior">
///       <span class="num">051</span>
///       <a href="/eboibfront/null/2026/12264/">BOIB Núm 051 - 21 / abril / 2026</a>
///     </div>
///   </div>
/// </li>
/// ```
fn parse_eboib_search_html(body: &str, keywords: &[String]) -> Result<Vec<ScrapedItem>> {
    let document = Html::parse_document(body);
    
    // Selectores basados en estructura real del eboibfront
    let li_selector = Selector::parse("li").unwrap();
    let caja_selector = Selector::parse("div.caja").unwrap();
    let interior_selector = Selector::parse("div.interior").unwrap();
    let num_selector = Selector::parse("span.num").unwrap();
    let link_selector = Selector::parse("a[href]").unwrap();
    
    let mut items = Vec::new();
    
    for li in document.select(&li_selector) {
        // Verificar que tenga div.caja > div.interior
        let caja = match li.select(&caja_selector).next() {
            Some(c) => c,
            None => continue,
        };
        
        let interior = match caja.select(&interior_selector).next() {
            Some(i) => i,
            None => continue,
        };
        
        // Extraer número de BOIB
        let num = interior
            .select(&num_selector)
            .next()
            .map(|n| n.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        
        // Extraer enlace y texto
        let link_elem = match interior.select(&link_selector).next() {
            Some(a) => a,
            None => continue,
        };
        
        let href = link_elem
            .value()
            .attr("href")
            .unwrap_or("");
        
        let link_text = link_elem.text().collect::<String>().trim().to_string();
        
        // Construir URL absoluta (corregir /null/ a /ca/ si es necesario)
        let url = if href.starts_with("http") {
            href.to_string()
        } else {
            let corrected_href = href.replace("/null/", "/ca/");
            format!("{}{}", CAIB_EBOIB_BASE, corrected_href)
        };
        
        // Construir título
        let title = if num.is_empty() {
            link_text.clone()
        } else {
            format!("BOIB Núm {} - {}", num, link_text.replace(&format!("BOIB Núm {} - ", num), ""))
        };
        
        // Filtrar por keywords si están definidas
        if !keywords.is_empty() {
            let search_text = format!("{} {}", title, link_text).to_lowercase();
            if !keywords.iter().any(|kw| search_text.contains(kw)) {
                continue;
            }
        }
        
        // Crear ID externo a partir del número de BOIB
        let external_id = format!("caib-boib-{}", num);
        
        tracing::debug!("CAIB: Item encontrado - {} -> {}", title, url);
        
        items.push(ScrapedItem {
            title: Some(title),
            description: Some(link_text),
            url: Some(url),
            external_id: Some(external_id),
            raw_data: None,
            published_at: None,
        });
    }
    
    tracing::info!("CAIB: parse_eboib_search_html encontró {} items", items.len());
    Ok(items)
}

/// Scrapea el RSS del BOIB
async fn scrape_eboib_rss(client: &Client, keywords: &[String]) -> Result<Vec<ScrapedItem>> {
    tracing::debug!("CAIB: GET {}", CAIB_EBOIB_RSS_URL);
    
    let resp = client
        .get(CAIB_EBOIB_RSS_URL)
        .send()
        .await
        .context("Failed to fetch eboibfront RSS")?;
    
    let body = resp.text().await.context("Failed to read RSS response")?;
    tracing::debug!("CAIB: RSS devolvió {} bytes", body.len());
    
    parse_eboib_rss(&body, keywords)
}

/// Parsea el RSS del eboibfront
/// 
/// Estructura:
/// ```xml
/// <item>
///   <title>BOIB Núm 051/2026</title>
///   <link>https://www.caib.es/eboibfront/ca/2026/12264</link>
///   <description />
///   <pubDate>Tue, 21 Apr 2026 06:30:00 GMT</pubDate>
/// </item>
/// ```
fn parse_eboib_rss(body: &str, keywords: &[String]) -> Result<Vec<ScrapedItem>> {
    let channel = rss::Channel::read_from(body.as_bytes())
        .map_err(|e| anyhow::anyhow!("Error parseando RSS del CAIB: {}", e))?;
    
    let mut items = Vec::new();
    
    for item in channel.items() {
        let title = item.title().unwrap_or("").to_string();
        let url = item.link().unwrap_or("").to_string();
        
        let published_at = item.pub_date().and_then(|d| {
            chrono::DateTime::parse_from_rfc2822(d)
                .ok()
                .map(|dt| dt.naive_utc())
        });
        
        if title.is_empty() || url.is_empty() {
            continue;
        }
        
        // Filtrar por keywords si estan definidas
        if !keywords.is_empty() {
            let search_text = title.to_lowercase();
            if !keywords.iter().any(|kw| search_text.contains(kw)) {
                continue;
            }
        }
        
        // Extraer numero para external_id
        let external_id = title
            .to_lowercase()
            .replace("boib num ", "caib-boib-")
            .replace("/", "-");
        
        tracing::debug!("CAIB: RSS item - {} -> {}", title, url);
        
        items.push(ScrapedItem {
            title: if title.is_empty() { None } else { Some(title) },
            description: None,
            url: if url.is_empty() { None } else { Some(url) },
            external_id: Some(external_id),
            raw_data: None,
            published_at,
        });
    }
    
    tracing::info!("CAIB: parse_eboib_rss encontro {} items", items.len());
    Ok(items)
}

/// Scrapea el portal principal (fallback)
async fn scrape_portal_principal(client: &Client, keywords: &[String]) -> Result<Vec<ScrapedItem>> {
    let url = CAIB_PORTAL_URL;
    tracing::debug!("CAIB: GET {}", url);
    
    let resp = client
        .get(url)
        .send()
        .await
        .context("Failed to fetch CAIB portal")?;
    
    let body = resp.text().await?;
    
    // Verificar si es página de error/autenticación
    if body.contains("Mòdul d'autenticació") || body.contains("autenticaci") {
        tracing::warn!("CAIB: Portal principal requiere autenticación");
        return Ok(Vec::new());
    }
    
    // Buscar enlaces a licitaciones en el HTML
    let document = Html::parse_document(&body);
    // Intentar selectores comunes
    let selectors = [
        "a[href*='licit']",
        "a[href*='contract']", 
        "a[href*='expedient']",
    ];
    
    let mut items = Vec::new();
    
    for sel_str in &selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            for link in document.select(&sel) {
                if let Some(href) = link.value().attr("href") {
                    let text = link.text().collect::<String>().trim().to_string();
                    
                    if text.is_empty() {
                        continue;
                    }
                    
                    // Filtrar por keywords
                    if !keywords.is_empty() {
                        let search_text = text.to_lowercase();
                        if !keywords.iter().any(|kw| search_text.contains(kw)) {
                            continue;
                        }
                    }
                    
                    let url = if href.starts_with("http") {
                        href.to_string()
                    } else {
                        format!("https://www.caib.es{}", href)
                    };
                    
                    let external_id = url.split('/').last().unwrap_or("").to_string();
                    
                    items.push(ScrapedItem {
                        title: Some(text.clone()),
                        description: Some(text),
                        url: Some(url),
                        external_id: Some(external_id),
                        raw_data: None,
                        published_at: None,
                    });
                }
            }
        }
    }
    
    tracing::info!("CAIB: scrape_portal_principal encontró {} items", items.len());
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_eboib_search_html() {
        let html = r#"
        <html><body>
        <ul>
            <li>
                <div class="caja">
                    <div class="interior">
                        <span class="num">051</span>
                        <a href="/eboibfront/null/2026/12264/">BOIB Núm 051 - 21 / abril / 2026</a>
                    </div>
                </div>
            </li>
            <li>
                <div class="caja">
                    <div class="interior">
                        <span class="num">050</span>
                        <a href="/eboibfront/null/2026/12263/">BOIB Núm 050 - 18 / abril / 2026</a>
                    </div>
                </div>
            </li>
        </ul>
        </body></html>
        ""#;
        
        let items = parse_eboib_search_html(html, &[]).unwrap();
        assert_eq!(items.len(), 2);
        assert!(items[0].url.as_ref().unwrap().contains("/ca/")); // Verificar corrección de URL
    }
    
    #[test]
    fn test_parse_eboib_rss() {
        let rss = r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
            <channel>
                <item>
                    <title>BOIB Núm 051/2026</title>
                    <link>https://www.caib.es/eboibfront/ca/2026/12264</link>
                    <pubDate>Tue, 21 Apr 2026 06:30:00 GMT</pubDate>
                </item>
            </channel>
        </rss>
        ""#;
        
        let items = parse_eboib_rss(rss, &[]).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title.as_ref().unwrap(), "BOIB Núm 051/2026");
    }
    
    #[test]
    fn test_keyword_filtering() {
        let html = r#"
        <html><body>
        <ul>
            <li>
                <div class="caja">
                    <div class="interior">
                        <span class="num">051</span>
                        <a href="/eboibfront/null/2026/12264/">BOIB Núm 051 - 21 / abril / 2026</a>
                    </div>
                </div>
            </li>
            <li>
                <div class="caja">
                    <div class="interior">
                        <span class="num">050</span>
                        <a href="/eboibfront/null/2026/12263/">Licitación obra pública</a>
                    </div>
                </div>
            </li>
        </ul>
        </body></html>
        ""#;
        
        let keywords = vec!["licitación".to_string(), "obra".to_string()];
        let items = parse_eboib_search_html(html, &keywords).unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].title.as_ref().unwrap().contains("Licitación"));
    }
}
