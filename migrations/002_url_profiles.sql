-- Perfiles de URLs aprendidos automáticamente
CREATE TABLE IF NOT EXISTS url_profiles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    domain TEXT NOT NULL UNIQUE,
    title_selector TEXT,
    item_selector TEXT,
    link_selector TEXT,
    description_selector TEXT,
    sample_url TEXT,
    confidence INTEGER NOT NULL DEFAULT 0, -- 0-100
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_url_profiles_domain ON url_profiles(domain);
