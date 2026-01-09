use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub admin: AdminConfig,
    pub proxy: ProxyConfig,
    pub auth: AuthConfig,
    pub database: DatabaseConfig,
    pub logging: LoggingConfig,
    #[serde(default = "default_timeout")]
    pub default_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProxyConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_path")]
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    pub directory: String,
    pub max_size_bytes: u64,
    pub retention_days: u32,
}

fn default_timeout() -> u64 {
    30
}

fn default_db_path() -> String {
    "./proxy.db".to_string()
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let mut config: Config = serde_yaml::from_str(&content)?;

        // 环境变量覆盖配置
        config.apply_env_overrides();

        Ok(config)
    }

    fn apply_env_overrides(&mut self) {
        // Admin 配置
        if let Ok(v) = env::var("PROXY_ADMIN_HOST") {
            self.admin.host = v;
        }
        if let Ok(v) = env::var("PROXY_ADMIN_PORT") {
            if let Ok(port) = v.parse() {
                self.admin.port = port;
            }
        }

        // Proxy 配置
        if let Ok(v) = env::var("PROXY_PROXY_HOST") {
            self.proxy.host = v;
        }
        if let Ok(v) = env::var("PROXY_PROXY_PORT") {
            if let Ok(port) = v.parse() {
                self.proxy.port = port;
            }
        }

        // 认证配置
        if let Ok(v) = env::var("PROXY_USERNAME") {
            self.auth.username = v;
        }
        if let Ok(v) = env::var("PROXY_PASSWORD") {
            self.auth.password = v;
        }

        // 数据库配置
        if let Ok(v) = env::var("PROXY_DB_PATH") {
            self.database.path = v;
        }

        // 日志配置
        if let Ok(v) = env::var("PROXY_LOG_DIR") {
            self.logging.directory = v;
        }
        if let Ok(v) = env::var("PROXY_LOG_MAX_SIZE") {
            if let Ok(size) = v.parse() {
                self.logging.max_size_bytes = size;
            }
        }
        if let Ok(v) = env::var("PROXY_LOG_RETENTION_DAYS") {
            if let Ok(days) = v.parse() {
                self.logging.retention_days = days;
            }
        }

        // 默认超时
        if let Ok(v) = env::var("PROXY_DEFAULT_TIMEOUT") {
            if let Ok(timeout) = v.parse() {
                self.default_timeout_secs = timeout;
            }
        }
    }
}
