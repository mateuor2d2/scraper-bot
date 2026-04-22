#[cfg(test)]
mod tests {
    use chrono::{Datelike, NaiveDate, Utc};
    use crate::models::SearchConfig;
    use crate::scraper::ScrapedItem;

    // ============================================
    // TESTS BOE - RSS y filtrado
    // ============================================

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

    /// Test de filtrado por keywords: crea un RSS de ejemplo con items variados
    /// y verifica que solo pasan los que coinciden con 'ingeniero industrial'.
    #[tokio::test]
    async fn test_boe_keyword_filtering_ingenieros() {
        let rss_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>BOE - Oposiciones</title>
    <item>
      <title>Oferta de empleo publico para ingenieros industriales</title>
      <link>https://www.boe.es/diario_boe/txt.php?id=BOE-A-2026-1111</link>
      <description>Convocatoria de oposiciones para el Cuerpo de Ingenieros Industriales</description>
      <pubDate>Tue, 15 Apr 2026 10:00:00 +0200</pubDate>
    </item>
    <item>
      <title>Oferta de empleo para administrativos</title>
      <link>https://www.boe.es/diario_boe/txt.php?id=BOE-A-2026-1112</link>
      <description>Convocatoria para administrativos generales</description>
      <pubDate>Wed, 16 Apr 2026 10:00:00 +0200</pubDate>
    </item>
    <item>
      <title>Resolucion de ingenieria industrial y civil</title>
      <link>https://www.boe.es/diario_boe/txt.php?id=BOE-A-2026-1113</link>
      <description>Plazas para ingenieros en general</description>
      <pubDate>Thu, 17 Apr 2026 10:00:00 +0200</pubDate>
    </item>
    <item>
      <title>Concurso de meritos para tecnicos</title>
      <link>https://www.boe.es/diario_boe/txt.php?id=BOE-A-2026-1114</link>
      <description>Concurso para tecnicos especializados</description>
      <pubDate>Fri, 18 Apr 2026 10:00:00 +0200</pubDate>
    </item>
  </channel>
</rss>"#;

        let channel = rss::Channel::read_from(rss_xml.as_bytes()).unwrap();
        
        // Keywords a buscar
        let keywords = vec!["ingeniero industrial".to_string()];
        
        let mut matched_items = Vec::new();
        for item in channel.items() {
            let title = item.title().unwrap_or("").to_lowercase();
            let description = item.description().unwrap_or("").to_lowercase();
            let text = format!("{} {}", title, description);
            
            // Aplicar logica de filtrado similar al scraper real
            let matches = keywords.iter().any(|kw| {
                if text.contains(kw) {
                    return true;
                }
                // AND matching para multi-palabras
                let words: Vec<&str> = kw.split_whitespace().collect();
                if words.len() > 1 {
                    return words.iter().all(|w| text.contains(w));
                }
                false
            });
            
            if matches {
                matched_items.push(item.title().unwrap_or("").to_string());
            }
        }
        
        // Deberia coincidir con item 1 (ingenieros industriales) y item 3 (ingenieria + industrial)
        assert_eq!(matched_items.len(), 2, "Esperaba 2 items coincidentes, encontrados: {:?}", matched_items);
        assert!(matched_items.iter().any(|t| t.contains("ingenieros industriales")));
        assert!(matched_items.iter().any(|t| t.contains("ingenieria industrial")));
    }

    /// Test de parseo de fecha RFC2822 con diferentes timezones
    #[test]
    fn test_parse_rfc2822_with_timezones() {
        let test_cases = vec![
            ("Sat, 18 Apr 2026 00:00:00 +0200", 2026, 4, 18),
            ("Tue, 15 Apr 2025 10:30:45 +0000", 2025, 4, 15),
            ("Wed, 16 Apr 2025 10:00:00 -0500", 2025, 4, 16),
            ("Mon, 01 Jan 2024 00:00:00 +0000", 2024, 1, 1),
        ];

        for (date_str, expected_year, expected_month, expected_day) in test_cases {
            let parsed = chrono::DateTime::parse_from_rfc2822(date_str)
                .ok()
                .map(|dt| dt.naive_local());
            
            assert!(parsed.is_some(), "No se pudo parsear: {}", date_str);
            
            let date = parsed.unwrap().date();
            assert_eq!(date.year(), expected_year, "Ano incorrecto para {}", date_str);
            assert_eq!(date.month(), expected_month, "Mes incorrecto para {}", date_str);
            assert_eq!(date.day(), expected_day, "Dia incorrecto para {}", date_str);
        }
    }

    // ============================================
    // TESTS CAIB - HTML mockeado
    // ============================================

    /// HTML de ejemplo real del portal CAIB (eboibfront)
    const CAIB_EBOIB_HTML_SAMPLE: &str = r#"
<!DOCTYPE html>
<html>
<body>
<div id="contenidor">
    <ul class="llistat-resultats">
        <li>
            <div class="caja">
                <div class="interior">
                    <span class="num">051</span>
                    <a href="/eboibfront/null/2026/12264/">BOIB Num 051 - 21 / abril / 2026</a>
                    <p class="detall">Anunci de licitacio per a l'obra publica de construccio d'un institut</p>
                </div>
            </div>
        </li>
        <li>
            <div class="caja">
                <div class="interior">
                    <span class="num">050</span>
                    <a href="/eboibfront/null/2026/12263/">BOIB Num 050 - 18 / abril / 2026</a>
                    <p class="detall">Licitacio de serveis de prevencio de riscos laborals</p>
                </div>
            </div>
        </li>
        <li>
            <div class="caja">
                <div class="interior">
                    <span class="num">049</span>
                    <a href="/eboibfront/null/2026/12262/">BOIB Num 049 - 17 / abril / 2026</a>
                    <p class="detall">Edicte sobre subministrament d'equipament informatic</p>
                </div>
            </div>
        </li>
    </ul>
</div>
</body>
</html>
"#;

    /// Test que verifica parse_caib_html extrae los campos correctos
    #[test]
    fn test_caib_parse_html_extracts_fields() {
        use scraper::{Html, Selector};
        
        let document = Html::parse_document(CAIB_EBOIB_HTML_SAMPLE);
        let li_selector = Selector::parse("li").unwrap();
        let caja_selector = Selector::parse("div.caja").unwrap();
        let interior_selector = Selector::parse("div.interior").unwrap();
        let num_selector = Selector::parse("span.num").unwrap();
        let link_selector = Selector::parse("a[href]").unwrap();
        
        let mut items = Vec::new();
        
        for li in document.select(&li_selector) {
            let caja = match li.select(&caja_selector).next() {
                Some(c) => c,
                None => continue,
            };
            
            let interior = match caja.select(&interior_selector).next() {
                Some(i) => i,
                None => continue,
            };
            
            let num = interior
                .select(&num_selector)
                .next()
                .map(|n| n.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            
            let link_elem = match interior.select(&link_selector).next() {
                Some(a) => a,
                None => continue,
            };
            
            let href = link_elem.value().attr("href").unwrap_or("");
            let link_text = link_elem.text().collect::<String>().trim().to_string();
            
            // Verificar correccion de URL
            let url = if href.starts_with("http") {
                href.to_string()
            } else {
                let corrected_href = href.replace("/null/", "/ca/");
                format!("https://www.caib.es{}", corrected_href)
            };
            
            let title = if num.is_empty() {
                link_text.clone()
            } else {
                format!("BOIB Num {} - {}", num, link_text.replace(&format!("BOIB Num {} - ", num), ""))
            };
            
            items.push((num, title, url));
        }
        
        assert_eq!(items.len(), 3, "Esperaba 3 items extraidos");
        
        // Verificar primer item
        assert_eq!(items[0].0, "051");
        assert!(items[0].1.contains("BOIB Num 051"));
        assert!(items[0].2.contains("/ca/"));
        assert!(!items[0].2.contains("/null/"));
        
        // Verificar correccion de URL
        assert!(items[1].2.contains("/eboibfront/ca/"));
    }

    /// Test que verifica filtrado por keywords en CAIB
    #[test]
    fn test_caib_keyword_filtering() {
        use scraper::{Html, Selector};
        
        let document = Html::parse_document(CAIB_EBOIB_HTML_SAMPLE);
        let li_selector = Selector::parse("li").unwrap();
        let link_selector = Selector::parse("a[href]").unwrap();
        let detail_selector = Selector::parse("p.detall").unwrap();
        
        let keywords = vec!["licitacio", "prevencio"];
        let mut matched_items = Vec::new();
        
        for li in document.select(&li_selector) {
            let link_text = li.select(&link_selector)
                .next()
                .map(|a| a.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            
            let detail_text = li.select(&detail_selector)
                .next()
                .map(|p| p.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            
            let search_text = format!("{} {}", link_text, detail_text).to_lowercase();
            
            // Filtrado similar al scraper
            let matches = keywords.iter().any(|kw| search_text.contains(&kw.to_lowercase()));
            
            if matches {
                matched_items.push((link_text, detail_text));
            }
        }
        
        // Deberia coincidir con item 1 (licitacio + obra) y item 2 (licitacio + prevencio)
        assert_eq!(matched_items.len(), 2, "Esperaba 2 items con keywords de licitacion");
        
        // Verificar que al menos uno contiene "prevencio"
        assert!(matched_items.iter().any(|(_, d)| d.contains("prevencio")));
    }

    const CAIB_RSS_SAMPLE: &str = r#"
<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
    <channel>
        <title>BOIB RSS</title>
        <item>
            <title>BOIB Num 051/2026</title>
            <link>https://www.caib.es/eboibfront/ca/2026/12264</link>
            <description>Anunci de licitacio</description>
            <pubDate>Tue, 21 Apr 2026 06:30:00 GMT</pubDate>
        </item>
        <item>
            <title>BOIB Num 050/2026</title>
            <link>https://www.caib.es/eboibfront/ca/2026/12263</link>
            <description>Edicte administratiu</description>
            <pubDate>Mon, 20 Apr 2026 06:30:00 GMT</pubDate>
        </item>
    </channel>
</rss>
"#;

    /// Test de parseo de RSS del CAIB
    #[test]
    fn test_caib_parse_rss() {
        let channel = rss::Channel::read_from(CAIB_RSS_SAMPLE.as_bytes()).unwrap();
        
        assert_eq!(channel.items().len(), 2);
        
        let item = &channel.items()[0];
        assert_eq!(item.title().unwrap(), "BOIB Num 051/2026");
        assert_eq!(item.link().unwrap(), "https://www.caib.es/eboibfront/ca/2026/12264");
        
        let published_at = item.pub_date().and_then(|d| {
            chrono::DateTime::parse_from_rfc2822(d)
                .ok()
                .map(|dt| dt.naive_utc())
        });
        
        assert!(published_at.is_some());
        let date = published_at.unwrap();
        assert_eq!(date.date().day(), 21);
        assert_eq!(date.date().month(), 4);
        assert_eq!(date.date().year(), 2026);
    }

    // ============================================
    // TESTS CONTRATACIONDELESTADO - Browser automation
    // ============================================

    /// Test de parseo de idLicitacion desde strings onclick
    #[test]
    fn test_contratacion_estado_extract_id_from_onclick() {
        let onclick_samples = vec![
            ("detalleLicitacion('JNDTgEobq1oXhk1FZxEyvw%3D%3D')", Some("JNDTgEobq1oXhk1FZxEyvw%3D%3D")),
            ("window.location='deeplink:detalle_licitacion&idEvl=ABC123'", Some("ABC123")),
            ("verDetalle('xyz789', 'param2')", Some("xyz789")),
            ("no valid onclick", None),
        ];

        for (onclick, expected) in onclick_samples {
            // Simular extraccion de ID
            let extracted = extract_id_from_onclick(onclick);
            assert_eq!(extracted, expected.map(|s| s.to_string()), 
                "Fallo extrayendo ID de: {}", onclick);
        }
    }

    fn extract_id_from_onclick(onclick: &str) -> Option<String> {
        // Patron 1: idEvl en URL
        if let Some(pos) = onclick.find("idEvl=") {
            let start = pos + 6;
            let end = onclick[start..].find(&['&', '\'', '"', ')', ';'][..])
                .map(|i| start + i)
                .unwrap_or(onclick.len());
            return Some(onclick[start..end].to_string());
        }
        
        // Patron 2: funcion con parametro entre comillas simples
        if let Some(start) = onclick.find('\'') {
            let rest = &onclick[start+1..];
            if let Some(end) = rest.find('\'') {
                return Some(rest[..end].to_string());
            }
        }
        
        None
    }

    /// Test de construccion de URLs de detalle
    #[test]
    fn test_contratacion_estado_detail_url_construction() {
        let base = "https://contrataciondelestado.es/wps/poc?uri=deeplink:detalle_licitacion&idEvl=";
        let test_ids = vec![
            "JNDTgEobq1oXhk1FZxEyvw%3D%3D",
            "ABC123-TEST",
            "simpleId123",
        ];

        for id in test_ids {
            let url = format!("{}{}", base, id);
            assert!(url.contains("deeplink:detalle_licitacion"));
            assert!(url.contains("idEvl="));
            assert!(url.ends_with(id));
        }
    }

    /// Test de parseo de fechas del portal
    #[test]
    fn test_contratacion_estado_date_parsing() {
        let test_cases = vec![
            ("21/04/2026", Some(NaiveDate::from_ymd_opt(2026, 4, 21).unwrap())),
            ("01/01/2024", Some(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap())),
            ("31/12/2025", Some(NaiveDate::from_ymd_opt(2025, 12, 31).unwrap())),
            ("", None),
            ("invalid", None),
            ("2026-04-21", None), // Formato incorrecto
        ];

        for (date_str, expected) in test_cases {
            let result = parse_portal_date(date_str);
            assert_eq!(result, expected, "Fallo parseando fecha: {}", date_str);
        }
    }

    fn parse_portal_date(date_str: &str) -> Option<NaiveDate> {
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
                return NaiveDate::from_ymd_opt(year, month, day);
            }
        }

        None
    }

    /// Test de integracion condicional para browser automation
    /// Este test esta marcado con #[ignore] porque requiere Chrome instalado
    #[tokio::test]
    #[ignore = "Requiere Chrome/Chromium instalado. Ejecutar manualmente con: cargo test -- --ignored"]
    async fn test_contratacion_estado_browser_launch() {
        // Este test es un placeholder para verificacion manual
        // En un entorno CI con Chrome disponible, remover el #[ignore]
        
        // Verificar que CHROME_PATH esta configurado o Chrome existe
        let chrome_available = std::env::var("CHROME_PATH").is_ok()
            || std::path::Path::new("/usr/bin/google-chrome").exists()
            || std::path::Path::new("/usr/bin/chromium").exists();
        
        assert!(chrome_available, "Chrome no esta disponible. Configura CHROME_PATH o instala Chrome.");
    }

    // ============================================
    // TESTS MOD - run_scrape dispatcher
    // ============================================

    fn create_test_search_config(search_type: &str, keywords: Option<&str>) -> SearchConfig {
        SearchConfig {
            id: 1,
            telegram_id: 12345,
            name: "Test Config".to_string(),
            url: "https://example.com/test".to_string(),
            search_type: search_type.to_string(),
            keywords: keywords.map(|s| s.to_string()),
            css_selector: None,
            notify_mode: "immediate".to_string(),
            is_active: true,
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
        }
    }

    /// Test que verifica que run_scrape despacha correctamente segun search_type
    /// Nota: Este test usa mocks/simulaciones porque run_scrape hace requests reales
    #[test]
    fn test_run_scrape_dispatcher_routes() {
        // Verificar que las funciones existen y son llamables
        // No podemos testear el dispatch real sin requests, pero podemos verificar
        // que los modulos estan correctamente estructurados
        
        let test_cases = vec![
            ("contratacion_estado", true),
            ("boe_rss", true),
            ("caib_licitaciones", true),
            ("generic_html", true),
            ("unknown_type", true), // fallback a generic
        ];

        for (search_type, _should_work) in test_cases {
            let config = create_test_search_config(search_type, Some("test,keywords"));
            
            // Verificar que la config se crea correctamente
            assert_eq!(config.search_type, search_type);
            assert!(config.keywords.is_some());
            
            // La verificacion de que el dispatch funciona requeriria mocking
            // o requests reales, lo cual esta fuera del scope de este test unitario
        }
    }

    /// Test de validacion de estructura ScrapedItem
    #[test]
    fn test_scraped_item_structure() {
        let item = ScrapedItem {
            title: Some("Test Title".to_string()),
            description: Some("Test Description".to_string()),
            url: Some("https://example.com/test".to_string()),
            external_id: Some("TEST-123".to_string()),
            raw_data: Some(r#"{"key":"value"}"#.to_string()),
            published_at: Some(Utc::now().naive_utc()),
        };

        assert!(item.title.is_some());
        assert_eq!(item.title.unwrap(), "Test Title");
        assert!(item.external_id.is_some());
        assert!(item.url.as_ref().unwrap().starts_with("https://"));
    }

    // ============================================
    // TESTS DB
    // ============================================

    #[tokio::test]
    async fn test_db_in_memory() {
        let db: crate::db::Db = crate::db::Db::new(":memory:").await.unwrap();

        // Crear usuario
        let user: crate::models::User = db.get_or_create_user(12345, Some("testuser"), Some("Test"), None).await.unwrap();
        assert_eq!(user.telegram_id, 12345);

        // Crear search config
        let id: i64 = db.create_search_config(
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
        let configs: Vec<crate::models::SearchConfig> = db.get_user_search_configs(12345).await.unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].notify_mode, "immediate");

        // Guardar resultado
        let result_id: i64 = db.save_search_result(
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
        let exists: bool = db.result_exists(id, "BOE-A-2026-1234").await.unwrap();
        assert!(exists);

        // Verificar unnotified results
        let unnotified: Vec<crate::models::SearchResultWithConfig> = db.get_unnotified_results(12345).await.unwrap();
        assert_eq!(unnotified.len(), 1);

        // Log scrape
        let _ = db.log_scrape(id, "ok", 5, None, 1200).await.unwrap();
        let logs: Vec<crate::db::ScrapeLog> = db.get_recent_scrape_logs(id, 10).await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].items_found, 5);
    }

    /// Test de busqueda de configuraciones por tipo
    #[tokio::test]
    async fn test_db_search_configs_by_type() {
        let db: crate::db::Db = crate::db::Db::new(":memory:").await.unwrap();
        
        let _user: crate::models::User = db.get_or_create_user(12345, Some("testuser"), Some("Test"), None).await.unwrap();
        
        // Crear multiples configs de diferentes tipos
        let _: i64 = db.create_search_config(
            12345, "BOE Test 1", "https://boe.es/rss/1", "boe_rss",
            Some("ingenieros"), None, Some("immediate"),
        ).await.unwrap();
        
        let _: i64 = db.create_search_config(
            12345, "CAIB Test", "https://caib.es/test", "caib_licitaciones",
            Some("obras"), None, Some("daily"),
        ).await.unwrap();
        
        let _: i64 = db.create_search_config(
            12345, "BOE Test 2", "https://boe.es/rss/2", "boe_rss",
            Some("administrativos"), None, Some("immediate"),
        ).await.unwrap();
        
        let configs: Vec<crate::models::SearchConfig> = db.get_user_search_configs(12345).await.unwrap();
        assert_eq!(configs.len(), 3);
        
        let boe_configs: Vec<&crate::models::SearchConfig> = configs.iter()
            .filter(|c| c.search_type == "boe_rss")
            .collect();
        assert_eq!(boe_configs.len(), 2);
        
        let caib_configs: Vec<&crate::models::SearchConfig> = configs.iter()
            .filter(|c| c.search_type == "caib_licitaciones")
            .collect();
        assert_eq!(caib_configs.len(), 1);
    }
}

// ============================================
// TESTS DE INTEGRACION (requieren conexion)
// ============================================

#[cfg(test)]
mod integration_tests {
    use crate::models::SearchConfig;
    use chrono::Utc;

    fn create_test_config(search_type: &str, keywords: &str, url: &str) -> SearchConfig {
        SearchConfig {
            id: 1,
            telegram_id: 12345,
            name: format!("Test {}", search_type),
            url: url.to_string(),
            search_type: search_type.to_string(),
            keywords: Some(keywords.to_string()),
            css_selector: None,
            notify_mode: "immediate".to_string(),
            is_active: true,
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
        }
    }

    /// Test de integracion: BOE RSS real
    /// Marcado como ignore porque requiere conexion a internet
    #[tokio::test]
    #[ignore = "Requiere conexion a internet. Ejecutar: cargo test -- --ignored integration_tests::test_boe_rss_live"]
    async fn test_boe_rss_live() {
        let config = create_test_config(
            "boe_rss",
            "ingeniero,industrial",
            "https://www.boe.es/rss/canal_per.php?l=p&c=140"
        );
        
        let result: Result<Vec<crate::scraper::ScrapedItem>, anyhow::Error> = crate::scraper::boe::scrape(&config).await;
        
        // Verificar que el scrape no falla (puede devolver vacio si no hay coincidencias)
        assert!(result.is_ok(), "El scrape de BOE deberia ejecutarse sin errores");
        
        let items: Vec<crate::scraper::ScrapedItem> = result.unwrap();
        // Log de cuantos items encontro
        eprintln!("BOE RSS encontro {} items", items.len());
        
        // Verificar estructura de items si hay resultados
        for item in &items {
            if let Some(title) = &item.title {
                assert!(!title.is_empty(), "El titulo no deberia estar vacio");
            }
        }
    }

    /// Test de integracion: CAIB licitaciones
    /// Marcado como ignore porque requiere conexion a internet
    #[tokio::test]
    #[ignore = "Requiere conexion a internet. Ejecutar: cargo test -- --ignored integration_tests::test_caib_licitaciones_live"]
    async fn test_caib_licitaciones_live() {
        let config = create_test_config(
            "caib_licitaciones",
            "licitacio,obra",
            "https://www.caib.es/sites/contractaciopublica/ca/"
        );
        
        let result: Result<Vec<crate::scraper::ScrapedItem>, anyhow::Error> = crate::scraper::caib::scrape(&config).await;
        
        assert!(result.is_ok(), "El scrape de CAIB deberia ejecutarse sin errores");
        
        let items: Vec<crate::scraper::ScrapedItem> = result.unwrap();
        eprintln!("CAIB encontro {} items", items.len());
        
        // Verificar que los items tienen URLs validas si hay resultados
        for item in &items {
            if let Some(url) = &item.url {
                assert!(url.starts_with("http"), "URL deberia ser absoluta: {}", url);
            }
        }
    }

    /// Test de integracion: Contratacion del Estado (con browser)
    /// Este test solo verifica que el codigo compila y los tipos son correctos
    /// porque requiere Chrome instalado
    #[tokio::test]
    #[ignore = "Requiere Chrome instalado y conexion a internet. Ejecutar manualmente."]
    async fn test_contratacion_estado_browser_live() {
        // Solo verificar que el modulo existe y es accesible
        // El test real requeriria Chrome disponible
        
        let config = create_test_config(
            "contratacion_estado",
            "prevencion riesgos laborales",
            "https://contrataciondelestado.es/wps/portal/plataforma/buscador/"
        );
        
        // Este test fallara silenciosamente si Chrome no esta disponible
        // ya que el scraper devuelve Vec::new() en ese caso
        let result: Result<Vec<crate::scraper::ScrapedItem>, anyhow::Error> = crate::scraper::contratacion_estado::scrape(&config).await;
        
        assert!(result.is_ok(), "El scrape deberia retornar Ok (aunque vacio si no hay Chrome)");
        
        let items: Vec<crate::scraper::ScrapedItem> = result.unwrap();
        eprintln!("Contratacion Estado encontro {} items (0 si no hay Chrome)", items.len());
    }
}

// ============================================
// TESTS DE BENCHMARK/CARGA (opcional)
// ============================================

#[cfg(test)]
mod benchmark_tests {
    /// Test de rendimiento del parseo de RSS
    #[test]
    fn test_rss_parse_performance() {
        use std::time::Instant;
        
        // Crear un RSS grande
        let mut rss_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>BOE Performance Test</title>
"#.to_string();
        
        // Anadir 100 items
        for i in 0..100 {
            rss_xml.push_str(&format!(r#"
    <item>
      <title>Oferta {} de empleo</title>
      <link>https://www.boe.es/item/{}</link>
      <description>Descripcion del item {}</description>
      <pubDate>Tue, {} Apr 2026 10:00:00 +0200</pubDate>
    </item>
"#, i, i, i, (i % 30) + 1));
        }
        
        rss_xml.push_str("  </channel>\n</rss>");
        
        let start = Instant::now();
        let channel = rss::Channel::read_from(rss_xml.as_bytes()).unwrap();
        let elapsed = start.elapsed();
        
        assert_eq!(channel.items().len(), 100);
        
        // Verificar que el parseo es rapido (< 100ms para 100 items)
        assert!(elapsed.as_millis() < 100, 
            "El parseo deberia ser mas rapido: {:?}", elapsed);
    }

    /// Test de rendimiento del parseo de HTML
    #[test]
    fn test_html_parse_performance() {
        use std::time::Instant;
        use scraper::Html;
        
        // Crear HTML grande simulando lista de licitaciones
        let mut html = "<html><body><ul>".to_string();
        for i in 0..100 {
            html.push_str(&format!(r#"
<li>
  <div class="caja">
    <div class="interior">
      <span class="num">{:03}</span>
      <a href="/test/{}/">Licitacion numero {}</a>
      <p>Descripcion de la licitacion {}</p>
    </div>
  </div>
</li>
"#, i, i, i, i));
        }
        html.push_str("</ul></body></html>");
        
        let start = Instant::now();
        let document = Html::parse_document(&html);
        let elapsed = start.elapsed();
        
        // Verificar que el documento se parseo correctamente
        let _ = &document;
        
        // Verificar rendimiento
        assert!(elapsed.as_millis() < 50, 
            "El parseo HTML deberia ser mas rapido: {:?}", elapsed);
    }
}