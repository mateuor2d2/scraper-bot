use scraper_bot::models::SearchConfig;
use scraper_bot::scraper;
use chrono::NaiveDateTime;

fn make_config(search_type: &str, url: &str, keywords: &str) -> SearchConfig {
    SearchConfig {
        id: 0, telegram_id: 0, name: "test".into(), url: url.into(),
        search_type: search_type.into(),
        keywords: if keywords.is_empty() { None } else { Some(keywords.into()) },
        css_selector: None, notify_mode: "instant".into(), filters: None,
        is_active: true,
        created_at: NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
        updated_at: NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== TEST BORM (Obscura CLI) ===\n");
    let config = make_config("borm_murcia", "https://www.borm.es", "contratación,licitación,concurso");
    match scraper::run_scrape(&config).await {
        Ok(items) => {
            println!("✅ BORM: {} items", items.len());
            for item in items.iter().take(5) {
                println!("  • {}", item.title.as_deref().unwrap_or("(sin título)"));
            }
        }
        Err(e) => println!("❌ BORM: {}", e),
    }
    Ok(())
}
