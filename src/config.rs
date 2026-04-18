use serde::Deserialize;
use std::fs;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub bot: BotConfig,
    pub database: DatabaseConfig,
    pub stripe: StripeConfig,
    pub scheduler: SchedulerConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BotConfig {
    pub name: String,
    pub description: String,
    pub admins: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DatabaseConfig {
    Sqlite { path: String },
}

impl DatabaseConfig {
    pub fn path(&self) -> String {
        match self {
            DatabaseConfig::Sqlite { path } => path.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StripeConfig {
    pub secret_key: String,
    pub publishable_key: String,
    pub webhook_secret: String,
    pub base_url: String,
    #[serde(default = "default_true")]
    pub test_mode: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchedulerConfig {
    pub daily_report_hour: u32,
    pub run_scrapes_interval_minutes: u64,
}

fn default_true() -> bool {
    true
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let content = fs::read_to_string("config.toml")?;
        let expanded = Self::expand_env_vars(&content);
        let config: Config = toml::from_str(&expanded)?;
        Ok(config)
    }

    fn expand_env_vars(content: &str) -> String {
        use regex::Regex;
        use std::env;
        let re = Regex::new(r"\$\{(\w+)(?::-([^}]*))?\}").unwrap();
        re.replace_all(content, |caps: &regex::Captures| {
            let var_name = &caps[1];
            let default_val = caps.get(2).map(|m| m.as_str());
            match env::var(var_name) {
                Ok(val) => val,
                Err(_) => default_val.unwrap_or("").to_string(),
            }
        }).to_string()
    }
}
