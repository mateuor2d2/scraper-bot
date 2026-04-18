use std::sync::Arc;
use tokio::time::{sleep, Duration};
use chrono::{Local, Timelike};

use crate::db::Db;
use crate::scraper;

pub fn start_scheduler(db: Arc<Db>, bot: teloxide::Bot, interval_minutes: u64, report_hour: u32) {
    tokio::spawn(async move {
        loop {
            let now = Local::now();
            let current_hour = now.hour();

            // Enviar informes diarios a la hora configurada
            if current_hour == report_hour && now.minute() < 5 {
                if let Err(e) = run_daily_reports(&db, &bot).await {
                    tracing::error!("Error en informes diarios: {}", e);
                }
            }

            // Ejecutar scrapes activos
            if let Err(e) = run_all_scrapes(&db, &bot).await {
                tracing::error!("Error ejecutando scrapes: {}", e);
            }

            sleep(Duration::from_secs(interval_minutes * 60)).await;
        }
    });
}

async fn run_all_scrapes(db: &Db, bot: &teloxide::Bot) -> anyhow::Result<()> {
    use teloxide::prelude::*;
    use teloxide::types::ParseMode;

    let configs = db.get_active_search_configs().await?;
    for config in configs {
        let start = std::time::Instant::now();
        let mut items_found = 0i32;
        let mut status = "ok";
        let mut error_msg: Option<String> = None;

        match scraper::run_scrape(&config).await {
            Ok(items) => {
                items_found = items.len() as i32;
                for item in items {
                    let external_id = item.external_id.as_deref().unwrap_or("");
                    if external_id.is_empty() {
                        // sin ID externo, guardamos siempre (podria duplicar)
                        let _ = db.save_search_result(
                            config.id,
                            item.title.as_deref(),
                            item.description.as_deref(),
                            item.url.as_deref(),
                            None,
                            item.raw_data.as_deref(),
                            item.published_at,
                        ).await;
                        continue;
                    }
                    match db.result_exists(config.id, external_id).await {
                        Ok(false) => {
                            if let Err(e) = db.save_search_result(
                                config.id,
                                item.title.as_deref(),
                                item.description.as_deref(),
                                item.url.as_deref(),
                                Some(external_id),
                                item.raw_data.as_deref(),
                                item.published_at,
                            ).await {
                                tracing::error!("Error guardando resultado: {}", e);
                            }
                        }
                        Ok(true) => {}
                        Err(e) => tracing::error!("Error verificando resultado: {}", e),
                    }
                }

                // Notificacion inmediata si corresponde
                if config.notify_mode == "immediate" || config.notify_mode == "both" {
                    if let Err(e) = send_immediate_notifications(db, bot, config.id, config.telegram_id).await {
                        tracing::error!("Error notificacion inmediata para config {}: {}", config.id, e);
                    }
                }
            }
            Err(e) => {
                status = "error";
                error_msg = Some(e.to_string());
                tracing::error!("Error scrapeando config {}: {}", config.id, e);
            }
        }

        let duration_ms = start.elapsed().as_millis() as i64;
        db.log_scrape(config.id, status, items_found, error_msg.as_deref(), duration_ms).await.ok();
    }
    Ok(())
}

async fn send_immediate_notifications(
    db: &Db,
    bot: &teloxide::Bot,
    search_config_id: i64,
    telegram_id: i64,
) -> anyhow::Result<()> {
    use teloxide::prelude::*;
    use teloxide::types::ParseMode;

    let results = db.get_unnotified_results_by_config(search_config_id).await?;
    if results.is_empty() {
        return Ok(());
    }

    let mut msg = format!(
        "\u{1f6a8} <b>Alerta inmediata</b>\n\n<b>Búsqueda:</b> {}\n<b>Nuevos resultados:</b> {}\n\n",
        results.first().map(|r| r.config_name.as_str()).unwrap_or("Desconocida"),
        results.len()
    );

    let mut ids = Vec::new();
    for result in &results[..std::cmp::min(5, results.len())] {
        ids.push(result.id);
        let title = result.title.as_deref().unwrap_or("Sin título");
        let url = result.url.as_deref().unwrap_or("#");
        msg.push_str(&format!(
            "\u{2705} <a href=\"{}\">{}</a>\n\n",
            url, title
        ));
    }

    if results.len() > 5 {
        msg.push_str(&format!("...y {} más\n", results.len() - 5));
    }

    if let Err(e) = bot
        .send_message(ChatId(telegram_id), msg)
        .parse_mode(ParseMode::Html)
        .disable_web_page_preview(true)
        .await
    {
        tracing::error!("Error enviando alerta inmediata a {}: {}", telegram_id, e);
    } else {
        db.mark_results_notified(ids).await.ok();
    }

    Ok(())
}

async fn run_daily_reports(db: &Db, bot: &teloxide::Bot) -> anyhow::Result<()> {
    use teloxide::prelude::*;
    use teloxide::types::ParseMode;

    let today = Local::now().naive_local().date();
    let users = sqlx::query_as::<_, crate::models::User>("SELECT * FROM users WHERE is_active = TRUE")
        .fetch_all(&db.pool)
        .await?;

    for user in users {
        let results = db.get_unnotified_results(user.telegram_id).await?;
        if results.is_empty() {
            continue;
        }

        let mut msg = format!(
            "\u{1f4ca} <b>Informe diario {}</b>\n\nTienes <b>{}</b> nuevos resultados:\n\n",
            today,
            results.len()
        );

        let mut ids = Vec::new();
    for result in &results[..std::cmp::min(10, results.len())] {
        ids.push(result.id);
        let title = result.title.as_deref().unwrap_or("Sin título");
        let url = result.url.as_deref().unwrap_or("#");
        msg.push_str(&format!(
            "\u{1f50d} <b>{}</b>\n<a href=\"{}\">{}</a>\n\n",
            result.config_name, url, title
        ));
    }

        if results.len() > 10 {
            msg.push_str(&format!("...y {} más\n", results.len() - 10));
        }

        if let Err(e) = bot
            .send_message(ChatId(user.telegram_id), msg)
            .parse_mode(ParseMode::Html)
            .disable_web_page_preview(true)
            .await
        {
            tracing::error!("Error enviando informe a {}: {}", user.telegram_id, e);
        } else {
            db.mark_results_notified(ids).await.ok();
            db.record_daily_report(user.telegram_id, today, results.len() as i32).await.ok();
        }
    }

    Ok(())
}
