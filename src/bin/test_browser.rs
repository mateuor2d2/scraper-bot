use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use std::time::Duration;

/// Try to connect to Obscura CDP, or spawn it if not running.
async fn ensure_obscura_running() -> Option<String> {
    let port = std::env::var("OBSCURA_PORT").unwrap_or_else(|_| "9222".to_string());
    let url = format!("http://127.0.0.1:{}/json/version", port);

    if let Ok(resp) = reqwest::get(&url).await {
        if resp.status().is_success() {
            println!("Obscura already running on port {}", port);
            return Some(url);
        }
    }

    let obscura_path = std::env::var("OBSCURA_PATH")
        .unwrap_or_else(|_| "obscura".to_string());

    let mut cmd = tokio::process::Command::new(&obscura_path);
    cmd.args(["serve", "--port", &port]);
    if std::env::var("OBSCURA_STEALTH").is_ok() {
        cmd.arg("--stealth");
    }
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    match cmd.spawn() {
        Ok(_) => {
            println!("Spawned Obscura, waiting for it to start...");
            for _ in 0..10 {
                tokio::time::sleep(Duration::from_millis(200)).await;
                if let Ok(resp) = reqwest::get(&url).await {
                    if resp.status().is_success() {
                        println!("Obscura started on port {}", port);
                        return Some(url);
                    }
                }
            }
            eprintln!("Obscura started but not responding");
            None
        }
        Err(e) => {
            eprintln!("Could not spawn Obscura: {}", e);
            None
        }
    }
}

/// Launch browser: try Obscura first, fall back to Chrome.
async fn launch_browser() -> anyhow::Result<(Browser, chromiumoxide::handler::Handler)> {
    // Strategy 1: Explicit OBSCURA_URL
    if let Ok(obscura_url) = std::env::var("OBSCURA_URL") {
        println!("Connecting to Obscura at {}", obscura_url);
        return Browser::connect(&obscura_url).await
            .map_err(|e| anyhow::anyhow!("Failed to connect to Obscura at {}: {}", obscura_url, e));
    }

    // Strategy 2: Auto-discover or spawn Obscura
    if let Some(url) = ensure_obscura_running().await {
        println!("Connecting to auto-discovered Obscura at {}", url);
        return Browser::connect(&url).await
            .map_err(|e| anyhow::anyhow!("Failed to connect to Obscura at {}: {}", url, e));
    }

    // Strategy 3: Fallback to local Chrome
    println!("Obscura not available, falling back to local Chrome launch");
    let config = BrowserConfig::builder()
        .build()
        .map_err(|e| anyhow::anyhow!("Browser config error: {}", e))?;
    Browser::launch(config).await
        .map_err(|e| anyhow::anyhow!("Failed to launch Chrome: {}", e))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    println!("=== Test Browser Automation - ContratacionDelEstado.es ===\n");

    // Launch browser (Obscura or Chrome fallback)
    println!("Launching browser...");
    let (mut browser, mut handler) = launch_browser().await?;

    // Spawn handler task
    tokio::spawn(async move {
        while let Some(h) = handler.next().await {
            if let Err(e) = h {
                eprintln!("Handler error: {:?}", e);
                break;
            }
        }
    });

    // Navigate to the search page
    println!("Navigating to buscadores page...");
    let page = browser.new_page("https://contrataciondelestado.es/wps/portal/plataforma/buscadores").await?;
    
    // Wait for page to load
    tokio::time::sleep(Duration::from_secs(3)).await;

    println!("Clicking on 'Bids' (Licitaciones)...");
    
    // Use JavaScript to click the Bids link
    let click_js = r##"
        (function() {
            const articles = document.querySelectorAll('article');
            for (const article of articles) {
                const heading = article.querySelector('h3');
                if (heading && heading.textContent.includes('Bids')) {
                    const link = article.querySelector("a[href='#']");
                    if (link) {
                        link.click();
                        return 'Clicked Bids link';
                    }
                }
            }
            const links = document.querySelectorAll('a');
            for (const link of links) {
                if (link.textContent.includes('Bids') && link.getAttribute('href') === '#') {
                    link.click();
                    return 'Clicked Bids link (fallback)';
                }
            }
            return 'Bids link not found';
        })()
    "##;
    let result = page.evaluate(click_js).await?;
    println!("Click result: {:?}", result.value());

    // Wait for form to load
    tokio::time::sleep(Duration::from_secs(3)).await;

    println!("Filling search form with 'prevención'...");
    
    // Type in the search field using the field ID we identified
    let fill_js = r##"
        (function() {
            const input = document.getElementById("viewns_Z7_AVEQAI930OBRD02JPMTPG21004_:form1:text71ExpMAQ");
            if (input) {
                input.value = "prevenci\u00f3n";
                input.dispatchEvent(new Event('input', { bubbles: true }));
                input.dispatchEvent(new Event('change', { bubbles: true }));
                return 'Filled File field with: ' + input.value;
            }
            return 'File input not found';
        })()
    "##;
    let type_result = page.evaluate(fill_js).await?;
    println!("Fill result: {:?}", type_result.value());

    // Click the Search button
    println!("Clicking Search button...");
    let search_js = r##"
        (function() {
            const btn = document.getElementById("viewns_Z7_AVEQAI930OBRD02JPMTPG21004_:form1:button1");
            if (btn) {
                btn.click();
                return 'Clicked Search button by ID';
            }
            const inputs = document.querySelectorAll('input[type="submit"]');
            for (const input of inputs) {
                if (input.value === 'Search' || input.value.includes('Buscar')) {
                    input.click();
                    return 'Clicked Search button (by value: ' + input.value + ')';
                }
            }
            return 'Search button not found';
        })()
    "##;
    let search_result = page.evaluate(search_js).await?;
    println!("Search click result: {:?}", search_result.value());

    // Wait for results to load
    println!("Waiting for results...");
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Extract results
    println!("\n=== Extracting results ===");
    let extract_js = r##"
        (function() {
            const table = document.getElementById('myTablaBusquedaCustom');
            if (!table) {
                return { error: 'Results table not found' };
            }

            const rows = table.querySelectorAll('tbody tr, tr');
            const items = [];
            
            const pagRow = rows[1];
            let paginationInfo = '';
            if (pagRow) {
                const pagCell = pagRow.querySelector('td');
                if (pagCell) {
                    paginationInfo = pagCell.textContent.trim();
                }
            }

            for (let i = 2; i < rows.length; i++) {
                const row = rows[i];
                const cells = row.querySelectorAll('td');
                if (cells.length >= 6) {
                    const titleCell = cells[0];
                    const link = titleCell.querySelector('a[onclick]');
                    
                    let idLicitacion = null;
                    if (link) {
                        const onclick = link.getAttribute('onclick') || '';
                        const match = onclick.match(/idLicitacion','(\d+)'/);
                        if (match) {
                            idLicitacion = match[1];
                        }
                    }

                    items.push({
                        title: titleCell.textContent.trim().substring(0, 100),
                        idLicitacion: idLicitacion,
                        type: cells[1]?.textContent?.trim() || '',
                        state: cells[2]?.textContent?.trim() || '',
                        amount: cells[3]?.textContent?.trim() || '',
                        date: cells[4]?.textContent?.trim() || '',
                        organization: cells[5]?.textContent?.trim()?.substring(0, 80) || ''
                    });
                }
            }

            return {
                pagination: paginationInfo,
                count: items.length,
                items: items.slice(0, 10)
            };
        })()
    "##;
    let results = page.evaluate(extract_js).await?;
    println!("Results JSON: {:?}", results.value());

    // Get page info
    println!("\n=== Page info ===");
    let title = page.get_title().await?;
    println!("Title: {:?}", title);

    let check_js = r##"
        (function() {
            const table = document.getElementById('myTablaBusquedaCustom');
            const searchForm = document.getElementById('viewns_Z7_AVEQAI930OBRD02JPMTPG21004_:form1:text71ExpMAQ');
            return {
                url: window.location.href,
                hasResultsTable: !!table,
                hasSearchForm: !!searchForm,
                bodySnippet: document.body.innerText.substring(0, 500)
            };
        })()
    "##;
    let content_check = page.evaluate(check_js).await?;
    println!("Page check: {:?}", content_check.value());

    // Close browser
    // With Browser::connect, close() sends CDP Browser.close but doesn't kill Obscura.
    browser.close().await?;
    println!("\nBrowser closed successfully.");

    Ok(())
}
