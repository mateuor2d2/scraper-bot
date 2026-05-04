-- Migración 004: Sistema de filtros avanzados para búsquedas

-- Añadir columna filters (JSON) a search_configs para filtros positivos/negativos
ALTER TABLE search_configs ADD COLUMN filters TEXT;

-- Tabla de reglas de filtro predefinidas (para UI futura)
CREATE TABLE IF NOT EXISTS filter_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    description TEXT,
    rule_type TEXT NOT NULL CHECK (rule_type IN ('include', 'exclude', 'regex_include', 'regex_exclude', 'date_after', 'date_before', 'status_active')),
    pattern TEXT NOT NULL,
    target_field TEXT NOT NULL DEFAULT 'any' CHECK (target_field IN ('any', 'title', 'description', 'url', 'raw_data')),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Índice para búsquedas por filtros
CREATE INDEX IF NOT EXISTS idx_search_configs_filters ON search_configs(filters);
