use serde::{Deserialize, Serialize};

/// Sistema de filtros avanzados para resultados de scraping.
/// 
/// Formato JSON almacenado en search_configs.filters:
/// {
///   "include": ["Baleares", "Mallorca"],
///   "exclude": ["Cáceres", "expirado"],
///   "status": "active"
/// }
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilterConfig {
    /// Palabras/frases que DEBEN aparecer (AND lógico)
    #[serde(default)]
    pub include: Vec<String>,
    /// Palabras/frases que NO deben aparecer
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Estado requerido (para portales que lo soporten)
    #[serde(default)]
    pub status: Option<String>,
}

impl FilterConfig {
    /// Parsea un string de filtros en formato simple:
    /// - `+Baleares,+Mallorca,-Cáceres,-expirado`
    /// - `Baleares,Mallorca` (solo positivos)
    /// - `-Cáceres,-Badajoz` (solo negativos)
    pub fn parse(input: &str) -> Self {
        let mut include = Vec::new();
        let mut exclude = Vec::new();

        for part in input.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if part.starts_with('-') || part.starts_with('!') {
                exclude.push(part[1..].trim().to_lowercase());
            } else if part.starts_with('+') {
                include.push(part[1..].trim().to_lowercase());
            } else {
                include.push(part.to_lowercase());
            }
        }

        Self {
            include,
            exclude,
            status: None,
        }
    }

    /// Convierte a string en formato legible
    pub fn to_display_string(&self) -> String {
        let mut parts = Vec::new();
        for i in &self.include {
            parts.push(format!("+{}", i));
        }
        for e in &self.exclude {
            parts.push(format!("-{}", e));
        }
        if parts.is_empty() {
            "(sin filtros)".to_string()
        } else {
            parts.join(", ")
        }
    }

    /// Verifica si un texto cumple con los filtros configurados
    pub fn matches(&self, text: &str) -> bool {
        let text_lower = text.to_lowercase();

        // Todos los includes deben estar presentes
        for inc in &self.include {
            if !text_lower.contains(inc) {
                return false;
            }
        }

        // Ningún exclude debe estar presente
        for exc in &self.exclude {
            if text_lower.contains(exc) {
                return false;
            }
        }

        true
    }

    /// Verifica si el item pasa los filtros, combinando título, descripción y URL
    pub fn matches_item(&self, title: Option<&str>, description: Option<&str>, url: Option<&str>) -> bool {
        let combined = format!(
            "{} {} {}",
            title.unwrap_or(""),
            description.unwrap_or(""),
            url.unwrap_or("")
        );
        self.matches(&combined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let f = FilterConfig::parse("Baleares, Mallorca");
        assert_eq!(f.include, vec!["baleares", "mallorca"]);
        assert!(f.exclude.is_empty());
    }

    #[test]
    fn test_parse_with_excludes() {
        let f = FilterConfig::parse("+Baleares, +Mallorca, -Cáceres, -expirado");
        assert_eq!(f.include, vec!["baleares", "mallorca"]);
        assert_eq!(f.exclude, vec!["cáceres", "expirado"]);
    }

    #[test]
    fn test_parse_only_excludes() {
        let f = FilterConfig::parse("-Cáceres, -Badajoz");
        assert!(f.include.is_empty());
        assert_eq!(f.exclude, vec!["cáceres", "badajoz"]);
    }

    #[test]
    fn test_matches_include() {
        let f = FilterConfig::parse("Baleares, coordinación");
        assert!(f.matches("Coordinación de seguridad en Baleares"));
        assert!(!f.matches("Coordinación de seguridad en Madrid"));
    }

    #[test]
    fn test_matches_exclude() {
        let f = FilterConfig::parse("coordinación, -Cáceres");
        assert!(f.matches("Coordinación de seguridad en Madrid"));
        assert!(!f.matches("Coordinación de seguridad en Cáceres"));
    }

    #[test]
    fn test_matches_combined() {
        let f = FilterConfig::parse("+Baleares, +coordinación, -expirado");
        assert!(f.matches("Coordinación de seguridad en Islas Baleares"));
        assert!(!f.matches("Coordinación de seguridad expirada en Baleares"));
        assert!(!f.matches("Coordinación de seguridad en Madrid"));
    }
}
