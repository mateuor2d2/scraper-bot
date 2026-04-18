-- Migración inicial: usuarios, búsquedas, resultados y pagos

CREATE TABLE IF NOT EXISTS users (
    telegram_id INTEGER PRIMARY KEY,
    username TEXT,
    first_name TEXT,
    last_name TEXT,
    phone TEXT,
    email TEXT,
    is_admin BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Configuración global de precios (solo admin modifica)
CREATE TABLE IF NOT EXISTS pricing (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    price_per_search_eur REAL NOT NULL DEFAULT 5.0,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
INSERT OR IGNORE INTO pricing (id, price_per_search_eur) VALUES (1, 5.0);

-- Búsquedas configuradas por cada cliente
CREATE TABLE IF NOT EXISTS search_configs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    telegram_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    search_type TEXT NOT NULL CHECK (search_type IN ('contratacion_estado', 'generic_html', 'generic_json')),
    keywords TEXT, -- palabras clave separadas por comas
    css_selector TEXT, -- selector CSS para extracción genérica
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (telegram_id) REFERENCES users(telegram_id) ON DELETE CASCADE
);

-- Resultados de scraping
CREATE TABLE IF NOT EXISTS search_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    search_config_id INTEGER NOT NULL,
    title TEXT,
    description TEXT,
    url TEXT,
    external_id TEXT, -- ID externo si existe (ej: expediente)
    raw_data TEXT, -- JSON con datos completos
    published_at DATETIME,
    scraped_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    notified BOOLEAN NOT NULL DEFAULT FALSE,
    FOREIGN KEY (search_config_id) REFERENCES search_configs(id) ON DELETE CASCADE
);

-- Suscripciones/pagos de usuarios
CREATE TABLE IF NOT EXISTS subscriptions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    telegram_id INTEGER NOT NULL UNIQUE,
    active_searches INTEGER NOT NULL DEFAULT 0,
    monthly_price_eur REAL NOT NULL DEFAULT 0.0,
    paid_until DATE,
    stripe_customer_id TEXT,
    stripe_subscription_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('active', 'pending', 'cancelled', 'past_due')),
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (telegram_id) REFERENCES users(telegram_id) ON DELETE CASCADE
);

-- Historial de pagos Stripe
CREATE TABLE IF NOT EXISTS payments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    telegram_id INTEGER NOT NULL,
    stripe_session_id TEXT,
    stripe_payment_intent_id TEXT,
    amount_eur REAL NOT NULL,
    searches_count INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'succeeded', 'failed', 'cancelled')),
    paid_at DATETIME,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (telegram_id) REFERENCES users(telegram_id) ON DELETE CASCADE
);

-- Registro de informes diarios enviados
CREATE TABLE IF NOT EXISTS daily_reports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    telegram_id INTEGER NOT NULL,
    report_date DATE NOT NULL,
    new_results_count INTEGER NOT NULL DEFAULT 0,
    sent_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (telegram_id) REFERENCES users(telegram_id) ON DELETE CASCADE
);

-- Índices
CREATE INDEX IF NOT EXISTS idx_search_configs_user ON search_configs(telegram_id);
CREATE INDEX IF NOT EXISTS idx_search_results_config ON search_results(search_config_id);
CREATE INDEX IF NOT EXISTS idx_search_results_notified ON search_results(notified, scraped_at);
CREATE INDEX IF NOT EXISTS idx_subscriptions_user ON subscriptions(telegram_id);
CREATE INDEX IF NOT EXISTS idx_payments_user ON payments(telegram_id);
CREATE INDEX IF NOT EXISTS idx_daily_reports_user_date ON daily_reports(telegram_id, report_date);
