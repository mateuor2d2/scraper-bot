-- Migración: tablas para API HTTP con API keys

CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    key_hash TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE,
    company TEXT NOT NULL,
    plan TEXT NOT NULL DEFAULT 'free' CHECK (plan IN ('free', 'pro')),
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    active BOOLEAN NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS api_searches (
    id TEXT PRIMARY KEY,
    api_key_id TEXT NOT NULL,
    boletin TEXT NOT NULL,
    keywords TEXT NOT NULL,
    date_from TEXT,
    date_to TEXT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed')),
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    completed_at DATETIME,
    result_json TEXT,
    error_message TEXT,
    FOREIGN KEY (api_key_id) REFERENCES api_keys(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS api_usage (
    id TEXT PRIMARY KEY,
    api_key_id TEXT NOT NULL,
    month TEXT NOT NULL,
    request_count INTEGER NOT NULL DEFAULT 0,
    limit_count INTEGER NOT NULL,
    UNIQUE(api_key_id, month),
    FOREIGN KEY (api_key_id) REFERENCES api_keys(id) ON DELETE CASCADE
);

-- Índices
CREATE INDEX IF NOT EXISTS idx_api_keys_email ON api_keys(email);
CREATE INDEX IF NOT EXISTS idx_api_keys_active ON api_keys(active);
CREATE INDEX IF NOT EXISTS idx_api_searches_key_id ON api_searches(api_key_id);
CREATE INDEX IF NOT EXISTS idx_api_searches_status ON api_searches(status);
CREATE INDEX IF NOT EXISTS idx_api_usage_key_month ON api_usage(api_key_id, month);
