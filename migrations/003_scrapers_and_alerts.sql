-- Soporte para nuevos scrapers y notificaciones inmediatas

-- Recrear tabla search_configs para incluir nuevos tipos y modo de notificacion
CREATE TABLE IF NOT EXISTS search_configs_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    telegram_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    search_type TEXT NOT NULL CHECK (search_type IN ('contratacion_estado', 'generic_html', 'generic_json', 'boe_rss', 'caib_licitaciones')),
    keywords TEXT,
    css_selector TEXT,
    notify_mode TEXT NOT NULL DEFAULT 'daily' CHECK (notify_mode IN ('immediate', 'daily', 'both')),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (telegram_id) REFERENCES users(telegram_id) ON DELETE CASCADE
);

INSERT INTO search_configs_new SELECT id, telegram_id, name, url, search_type, keywords, css_selector, 'daily', is_active, created_at, updated_at FROM search_configs;

DROP TABLE search_configs;
ALTER TABLE search_configs_new RENAME TO search_configs;

CREATE INDEX IF NOT EXISTS idx_search_configs_user ON search_configs(telegram_id);

-- Tabla de logs de scrapes para debug y monitoreo
CREATE TABLE IF NOT EXISTS scrape_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    search_config_id INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('ok', 'error', 'timeout')),
    items_found INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    duration_ms INTEGER,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (search_config_id) REFERENCES search_configs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_scrape_logs_config ON scrape_logs(search_config_id);
CREATE INDEX IF NOT EXISTS idx_scrape_logs_created ON scrape_logs(created_at);
