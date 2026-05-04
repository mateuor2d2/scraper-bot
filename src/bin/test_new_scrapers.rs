//! Test rápido de los 4 nuevos scrapers
//! cargo run --bin test_new_scrapers

use scraper_bot::models::SearchConfig;
use scraper_bot::scraper;
use chrono::NaiveDateTime;

fn make_config(search_type: &str, url: &str, keywords: &str) -> SearchConfig {
    SearchConfig {
        id: 0,
        telegram_id: 0,
        name: "test".into(),
        url: url.into(),
        search_type: search_type.into(),
        keywords: if keywords.is_empty() { None } else { Some(keywords.into()) },
        css_selector: None,
        notify_mode: "instant".into(),
        filters: None,
        is_active: true,
        created_at: NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
        updated_at: NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== TEST 4 NUEVOS SCRAPERS ===\n");

    // Test 1: BOCYL (RSS)
    println!("--- 1/4 BOCYL (Castilla y León) ---");
    let config = make_config("bocyl_rss", "https://bocyl.jcyl.es/rss.do?seccion=I", "contratación,licitación");
    match scraper::run_scrape(&config).await {
        Ok(items) => {
            println!("  ✅ BOCYL: {} items", items.len());
            for item in items.iter().take(3) {
                println!("     • {}", item.title.as_deref().unwrap_or("(sin título)"));
            }
        }
        Err(e) => println!("  ❌ BOCYL: {}", e),
    }

    // Test 2: DOE (RSS)
    println!("\n--- 2/4 DOE (Extremadura) ---");
    let config = make_config("doe_rss", "https://doe.juntaex.es/rss/rss.php?seccion=1", "contratación,licitación");
    match scraper::run_scrape(&config).await {
        Ok(items) => {
            println!("  ✅ DOE: {} items", items.len());
            for item in items.iter().take(3) {
                println!("     • {}", item.title.as_deref().unwrap_or("(sin título)"));
            }
        }
        Err(e) => println!("  ❌ DOE: {}", e),
    }

    // Test 3: BOC Canarias (RSS)
    println!("\n--- 3/4 BOC Canarias ---");
    let config = make_config("boc_canarias_rss", "https://www.gobiernodecanarias.org/boc/feeds/capitulo/disposiciones_generales.rss", "contratación,licitación");
    match scraper::run_scrape(&config).await {
        Ok(items) => {
            println!("  ✅ BOC Canarias: {} items", items.len());
            for item in items.iter().take(3) {
                println!("     • {}", item.title.as_deref().unwrap_or("(sin título)"));
            }
        }
        Err(e) => println!("  ❌ BOC Canarias: {}", e),
    }

    // Test 4: BORM Murcia (Obscura stealth)
    println!("\n--- 4/4 BORM Murcia (Obscura stealth) ---");
    println!("  (puede tardar 30s+ por headless browser)");
    let config = make_config("borm_murcia", "https://www.borm.es", "contratación,licitación");
    match scraper::run_scrape(&config).await {
        Ok(items) => {
            println!("  ✅ BORM: {} items", items.len());
            for item in items.iter().take(3) {
                println!("     • {}", item.title.as_deref().unwrap_or("(sin título)"));
            }
        }
        Err(e) => println!("  ❌ BORM: {}", e),
    }

    println!("\n=== FIN TEST ===");
    Ok(())
}
