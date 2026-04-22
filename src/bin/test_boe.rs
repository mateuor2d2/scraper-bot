use scraper_bot::models::SearchConfig;
use scraper_bot::scraper::boe;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    println!("=== Test BOE RSS Scraper - Ingenieros Industriales ===\n");

    // Test with the personal/oposiciones channel (better for job offers)
    let config = SearchConfig {
        id: 1,
        telegram_id: 12345,
        name: "Test BOE Ingenieros".to_string(),
        url: "https://www.boe.es/rss/canal_per.php?l=p&c=140".to_string(),
        search_type: "boe_rss".to_string(),
        keywords: Some("ingeniero industrial,ingeniería industrial".to_string()),
        css_selector: None,
        notify_mode: "immediate".to_string(),
        is_active: true,
        created_at: chrono::Local::now().naive_local(),
        updated_at: chrono::Local::now().naive_local(),
    };

    println!("URL: {}", config.url);
    println!("Keywords: {:?}", config.keywords);
    println!("---\n");

    match boe::scrape(&config).await {
        Ok(items) => {
            println!("✅ Total items found: {}\n", items.len());

            if items.is_empty() {
                println!("⚠️  No items found with current keywords.");
                println!("   Try with broader keywords like 'ingeniero' or 'industrial' separately.");
            } else {
                println!("=== First {} results ===", items.len().min(10));
                for (i, item) in items.iter().take(10).enumerate() {
                    println!("\n{}. {}", i + 1, item.title.as_deref().unwrap_or("(no title)"));
                    println!("   URL: {}", item.url.as_deref().unwrap_or("(no url)"));
                    if let Some(desc) = &item.description {
                        let short_desc = if desc.len() > 100 {
                            format!("{}...", &desc[..100])
                        } else {
                            desc.to_string()
                        };
                        println!("   Desc: {}", short_desc);
                    }
                    if let Some(date) = item.published_at {
                        println!("   Published: {}", date);
                    }
                }

                if items.len() > 10 {
                    println!("\n... and {} more", items.len() - 10);
                }
            }
        }
        Err(e) => {
            eprintln!("❌ Error scraping BOE: {}", e);
            return Err(e);
        }
    }

    // Also test with broader keywords
    println!("\n\n=== Test with broader keyword 'ingeniero' ===");
    let config2 = SearchConfig {
        id: 2,
        telegram_id: 12345,
        name: "Test BOE Ingenieros Broad".to_string(),
        url: "https://www.boe.es/rss/canal_per.php?l=p&c=140".to_string(),
        search_type: "boe_rss".to_string(),
        keywords: Some("ingeniero".to_string()),
        css_selector: None,
        notify_mode: "immediate".to_string(),
        is_active: true,
        created_at: chrono::Local::now().naive_local(),
        updated_at: chrono::Local::now().naive_local(),
    };

    match boe::scrape(&config2).await {
        Ok(items) => {
            println!("✅ Found {} items with keyword 'ingeniero'", items.len());
            for (i, item) in items.iter().take(5).enumerate() {
                println!("  {}. {}", i + 1, item.title.as_deref().unwrap_or("(no title)"));
            }
        }
        Err(e) => {
            eprintln!("❌ Error: {}", e);
        }
    }

    println!("\n\n=== Test with default s=2B URL (Section II.B) ===");
    let config3 = SearchConfig {
        id: 3,
        telegram_id: 12345,
        name: "Test BOE s=2B".to_string(),
        url: "https://www.boe.es/rss/boe.php?s=2B".to_string(),
        search_type: "boe_rss".to_string(),
        keywords: Some("ingeniero industrial,ingeniería industrial".to_string()),
        css_selector: None,
        notify_mode: "immediate".to_string(),
        is_active: true,
        created_at: chrono::Local::now().naive_local(),
        updated_at: chrono::Local::now().naive_local(),
    };

    match boe::scrape(&config3).await {
        Ok(items) => {
            println!("✅ Found {} items from s=2B", items.len());
        }
        Err(e) => {
            eprintln!("❌ Error: {}", e);
        }
    }

    Ok(())
}
