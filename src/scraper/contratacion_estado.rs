use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::js_protocol::runtime::EvaluateParams;
use futures::StreamExt;
use std::time::Duration;

use crate::models::SearchConfig;
use crate::scraper::ScrapedItem;

const BASE_URL: &str = "https://contrataciondelestado.es/wps/portal/plataforma/buscador/";
const DETAIL_URL_BASE: &str = "https://contrataciondelestado.es/wps/poc?uri=deeplink:detalle_licitacion&idEvl=";

/// Scraper para contrataciondelestado.es usando browser automation (chromiumoxide).
///
/// # Limitaciones conocidas:
/// - Requiere Chrome/Chromium instalado en el sistema o accesible via CDP.
/// - El portal usa JSF con ViewState dinámico, por lo que no funciona con HTTP simple.
/// - Los tiempos de carga pueden variar; se usan timeouts de 30s para navegación.
/// - El buscador del portal a veces devuelve documentos en lugar de licitaciones directamente.
///
/// # Flujo de scraping:
/// 1. Navega al buscador del portal.
/// 2. Introduce las keywords en el campo de búsqueda.
/// 3. Ejecuta la búsqueda haciendo click en "Buscar".
/// 4. Espera a que cargue la tabla de resultados.
/// 5. Extrae filas: título, descripción, idLicitacion (del URL de detalle), fecha.
pub async fn scrape(config: &SearchConfig) -> Result<Vec<ScrapedItem>> {
    let keywords = config
        .keywords
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<String>>();

    let keyword = keywords.first().cloned().unwrap_or_else(|| "prevención de riesgos".to_string());

    // Intentar lanzar el browser
    let (mut browser, handler) = match launch_browser().await {
        Some((b, h)) => (b, h),
        None => {
            tracing::warn!(
                "Chrome/Chromium no disponible. Scraper de contrataciondelestado.es no puede funcionar sin browser. \
                 Instala Chrome o configura CHROME_PATH para habilitar este scraper."
            );
            return Ok(Vec::new());
        }
    };

    // Spawn handler task
    let _handle = tokio::spawn(async move {
        let mut handler = handler;
        while let Some(h) = handler.next().await {
            match h {
                Ok(_) => continue,
                Err(_) => break,
            }
        }
    });

    let result = scrape_with_browser(&mut browser, &keyword).await;

    // Cerrar el browser
    if let Err(e) = browser.close().await {
        tracing::warn!("Error cerrando browser: {}", e);
    }

    result
}

async fn launch_browser() -> Option<(Browser, chromiumoxide::handler::Handler)> {
    // Buscar Chrome si no está en CHROME_PATH
    let chrome_path = std::env::var("CHROME_PATH")
        .ok()
        .or_else(|| find_chrome_binary());

    // Configuración del browser
    let mut builder = BrowserConfig::builder()
        .no_sandbox()
        .arg("--disable-dev-shm-usage")
        .arg("--disable-gpu")
        .arg("--disable-web-security")
        .arg("--disable-features=IsolateOrigins,site-per-process")
        .arg("--user-agent=Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .viewport(None);

    if let Some(path) = chrome_path {
        builder = builder.chrome_executable(path);
    }

    let config = match builder.build() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Error construyendo config del browser: {}", e);
            return None;
        }
    };

    match Browser::launch(config).await {
        Ok((browser, handler)) => Some((browser, handler)),
        Err(e) => {
            tracing::warn!("No se pudo lanzar Chrome: {}", e);
            None
        }
    }
}

fn find_chrome_binary() -> Option<String> {
    let possible_paths = [
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/usr/local/bin/chrome",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
        "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
    ];

    for path in &possible_paths {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    // Intentar encontrar via which/comando
    if let Ok(output) = std::process::Command::new("which")
        .args(&["google-chrome", "chromium", "chromium-browser"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if !line.is_empty() && std::path::Path::new(line).exists() {
                return Some(line.to_string());
            }
        }
    }

    None
}

async fn scrape_with_browser(browser: &mut Browser, keyword: &str) -> Result<Vec<ScrapedItem>> {
    let page = browser
        .new_page(BASE_URL)
        .await
        .context("No se pudo crear nueva página")?;

    // Esperar a que la página cargue
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Encontrar y llenar el campo de búsqueda
    let find_input_js = r#"
        (function() {
            const inputs = document.querySelectorAll('input[type="text"]');
            for (let input of inputs) {
                const id = input.id || '';
                const title = input.title || '';
                const placeholder = input.placeholder || '';
                if (id.includes('texto') || title.includes('buscar') || placeholder.includes('buscar')) {
                    return input.id || input.name || '';
                }
            }
            // Fallback: primer input de texto visible
            for (let input of inputs) {
                if (input.offsetParent !== null) {
                    return input.id || input.name || '';
                }
            }
            return '';
        })()
    "#;

    let input_result = page
        .evaluate(find_input_js)
        .await
        .context("Error evaluando JS para encontrar input")?;

    let input_id = input_result
        .value()
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if input_id.is_empty() {
        tracing::debug!("No se encontró input específico, intentando selector genérico");
    }

    // Llenar el campo de búsqueda
    let fill_js = format!(
        r#"
        (function() {{
            let input = document.getElementById('{}') || 
                        document.querySelector('input[type="text"]') ||
                        document.querySelector('input[name*="texto"]');
            if (input) {{
                input.value = '{}';
                input.dispatchEvent(new Event('input', {{ bubbles: true }}));
                input.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return true;
            }}
            return false;
        }})()
        "#,
        input_id.replace("'", "\\'"),
        keyword.replace("'", "\\'")
    );

    let fill_result = page
        .evaluate(fill_js)
        .await
        .context("Error llenando campo de búsqueda")?;

    if !fill_result.value().and_then(|v| v.as_bool()).unwrap_or(false) {
        tracing::warn!("No se pudo llenar el campo de búsqueda");
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Hacer click en el botón Buscar
    let click_js = r#"
        (function() {
            const buttons = document.querySelectorAll('input[type="submit"], button, input[type="button"]');
            for (let btn of buttons) {
                const text = (btn.value || btn.textContent || '').toLowerCase();
                if (text.includes('buscar') || text.includes('search')) {
                    btn.click();
                    return true;
                }
            }
            const form = document.querySelector('form');
            if (form) {
                form.submit();
                return true;
            }
            return false;
        })()
    "#;

    let click_result = page
        .evaluate(click_js)
        .await
        .context("Error haciendo click en buscar")?;

    if !click_result.value().and_then(|v| v.as_bool()).unwrap_or(false) {
        tracing::warn!("No se encontró botón de búsqueda");
    }

    // Esperar a que carguen los resultados
    let wait_js = r#"
        new Promise((resolve) => {
            const check = () => {
                const table = document.querySelector('table.tabla-detalle');
                const rows = document.querySelectorAll('table tr');
                if (table && rows.length > 1) {
                    resolve(true);
                } else {
                    setTimeout(check, 500);
                }
            };
            check();
            setTimeout(() => resolve(false), 25000);
        })
    "#;

    let params = EvaluateParams::builder()
        .expression(wait_js.to_string())
        .await_promise(true)
        .build()
        .map_err(|e| anyhow::anyhow!("Error construyendo params: {}", e))?;

    let has_results = page
        .evaluate(params)
        .await
        .map(|r| r.value().and_then(|v| v.as_bool()).unwrap_or(false))
        .unwrap_or(false);

    if !has_results {
        tracing::warn!("No se cargaron resultados después de esperar");
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Extraer resultados
    let extract_js = r#"
        (function() {
            const items = [];
            const rows = document.querySelectorAll('table.tabla-detalle tbody tr, table tr');

            for (let row of rows) {
                const cells = row.querySelectorAll('td');
                if (cells.length < 2) continue;

                const titleEl = cells[1].querySelector('a[id*="textColumnaTituloResultado"], a span.commandLink');
                const title = titleEl ? titleEl.textContent.trim() : '';

                const descEl = cells[1].querySelector('span[id*="textColumnaExpedienteResultado"], span.outputText');
                const description = descEl ? descEl.textContent.trim() : '';

                const dateEl = cells[2] ? cells[2].querySelector('span[id*="textColumnaDateResultado"]') : null;
                const dateStr = dateEl ? dateEl.textContent.trim() : '';

                const detailLink = cells[1].querySelector('a[href*="detalle_licitacion"]');
                let externalId = '';
                let detailUrl = '';

                if (detailLink) {
                    const href = detailLink.getAttribute('href') || '';
                    detailUrl = href;
                    const match = href.match(/idEvl=([^&]+)/);
                    if (match) {
                        externalId = match[1];
                    }
                }

                if (title || description || externalId) {
                    items.push({
                        title: title,
                        description: description,
                        date: dateStr,
                        external_id: externalId,
                        detail_url: detailUrl
                    });
                }
            }

            return JSON.stringify(items);
        })()
    "#;

    let extract_result = page
        .evaluate(extract_js)
        .await
        .context("Error extrayendo resultados")?;

    let items_json = extract_result
        .value()
        .and_then(|v| v.as_str())
        .unwrap_or("[]");

    let raw_items: Vec<serde_json::Value> = serde_json::from_str(items_json).unwrap_or_default();

    let mut items = Vec::new();

    for raw in raw_items {
        let title = raw.get("title").and_then(|v| v.as_str()).map(|s| s.to_string());
        let description = raw.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());
        let date_str = raw.get("date").and_then(|v| v.as_str()).unwrap_or("");
        let external_id = raw.get("external_id").and_then(|v| v.as_str()).map(|s| s.to_string());

        let url = external_id.as_ref().and_then(|id| {
            if id.is_empty() {
                None
            } else {
                Some(format!("{}{}", DETAIL_URL_BASE, id))
            }
        });

        let published_at = parse_date(date_str);

        items.push(ScrapedItem {
            title,
            description,
            url,
            external_id,
            raw_data: Some(raw.to_string()),
            published_at,
        });
    }

    tracing::info!(
        "Scraper contrataciondelestado.es encontró {} resultados para keyword: {}",
        items.len(),
        keyword
    );

    Ok(items)
}

fn parse_date(date_str: &str) -> Option<NaiveDateTime> {
    if date_str.is_empty() {
        return None;
    }

    let parts: Vec<&str> = date_str.split('/').collect();
    if parts.len() == 3 {
        if let (Ok(day), Ok(month), Ok(year)) = (
            parts[0].parse::<u32>(),
            parts[1].parse::<u32>(),
            parts[2].parse::<i32>(),
        ) {
            return chrono::NaiveDate::from_ymd_opt(year, month, day)
                .map(|d| d.and_hms_opt(0, 0, 0).unwrap_or_default());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_date() {
        assert!(parse_date("21/04/2026").is_some());
        assert!(parse_date("").is_none());
        assert!(parse_date("invalid").is_none());
    }

    #[test]
    fn test_detail_url_construction() {
        let id = "JNDTgEobq1oXhk1FZxEyvw%3D%3D";
        let url = format!("{}{}", DETAIL_URL_BASE, id);
        assert!(url.contains("idEvl="));
        assert!(url.contains(id));
    }
}
