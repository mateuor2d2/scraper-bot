use chromiumoxide::{Browser, BrowserConfig};
use futures_util::StreamExt;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    println!("=== Test Browser Automation - ContratacionDelEstado.es ===\n");

    // Launch browser
    println!("Launching Chrome browser...");
    let (mut browser, mut handler) = Browser::launch(
        BrowserConfig::builder()
            .build()
            .map_err(|e| anyhow::anyhow!("Browser config error: {}", e))?
    ).await?;

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
    browser.close().await?;
    println!("\nBrowser closed successfully.");

    Ok(())
}
