use reqwest;
use scraper::{Html, Selector};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = "https://news.ycombinator.com";
    println!("Fetching {}...", url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (compatible; ScraperBot/0.1)")
        .build()?;

    let body = client.get(url).send().await?.text().await?;
    let document = Html::parse_document(&body);

    let title_selector = Selector::parse(".titleline > a").unwrap();
    let subtext_selector = Selector::parse(".subtext").unwrap();

    let titles: Vec<_> = document.select(&title_selector).collect();
    let subtexts: Vec<_> = document.select(&subtext_selector).collect();

    println!("Found {} items\n", titles.len().min(subtexts.len()));

    for (i, (title, sub)) in titles.iter().zip(subtexts.iter()).take(5).enumerate() {
        let text = title.text().collect::<String>();
        let href = title.value().attr("href").unwrap_or("#");
        let meta = sub.text().collect::<String>().replace("\n", " ");
        println!("{}. {}\n   Link: {}\n   Meta: {}\n", i + 1, text, href, meta.trim());
    }

    Ok(())
}
