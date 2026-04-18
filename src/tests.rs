#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_boe_rss_parse_sample() {
        let rss_xml = r#"<?xml version="1.0" encoding="ISO-8859-1"?>
<rss version="2.0">
  <channel>
    <title>BOE</title>
    <item>
      <title>Resolucion de prueba</title>
      <link>https://www.boe.es/diario_boe/txt.php?id=BOE-A-2026-1234</link>
      <description>Descripcion de prueba</description>
      <guid isPermaLink="true">https://www.boe.es/boe/dias/2026/04/18/pdfs/BOE-A-2026-1234.pdf</guid>
      <pubDate>Sat, 18 Apr 2026 00:00:00 +0200</pubDate>
    </item>
  </channel>
</rss>"#;

        let channel = rss::Channel::read_from(rss_xml.as_bytes()).unwrap();
        assert_eq!(channel.items().len(), 1);
        let item = &channel.items()[0];
        assert_eq!(item.title().unwrap(), "Resolucion de prueba");
        assert!(item.link().unwrap().contains("boe.es"));
    }

    #[tokio::test]
    async fn test_db_in_memory() {
        let db = crate::db::Db::new(":memory:").await.unwrap();

        // Crear usuario
        let user = db.get_or_create_user(12345, Some("testuser"), Some("Test"), None).await.unwrap();
        assert_eq!(user.telegram_id, 12345);

        // Crear search config
        let id = db.create_search_config(
            12345,
            "Test BOE",
            "https://www.boe.es/rss/boe.php?s=1",
            "boe_rss",
            Some("obras, servicios"),
            None,
            Some("immediate"),
        ).await.unwrap();
        assert!(id > 0);

        // Verificar que se puede leer
        let configs = db.get_user_search_configs(12345).await.unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].notify_mode, "immediate");

        // Guardar resultado
        let result_id = db.save_search_result(
            id,
            Some("Titulo test"),
            Some("Desc test"),
            Some("https://boe.es/test"),
            Some("BOE-A-2026-1234"),
            None,
            None,
        ).await.unwrap();
        assert!(result_id > 0);

        // Verificar que no existe duplicado
        let exists = db.result_exists(id, "BOE-A-2026-1234").await.unwrap();
        assert!(exists);

        // Verificar unnotified results
        let unnotified = db.get_unnotified_results(12345).await.unwrap();
        assert_eq!(unnotified.len(), 1);

        // Log scrape
        db.log_scrape(id, "ok", 5, None, 1200).await.unwrap();
        let logs = db.get_recent_scrape_logs(id, 10).await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].items_found, 5);
    }
}
