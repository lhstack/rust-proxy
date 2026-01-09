use anyhow::Result;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::{Deserialize, Serialize};

/// 代理规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyRule {
    pub id: i64,
    pub name: String,
    pub source: String,
    pub target: String,
    pub timeout_secs: u64,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 系统配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub id: i64,
    pub key: String,
    pub value: String,
}

/// 数据库连接池管理器
#[derive(Clone)]
pub struct Database {
    pool: Pool<SqliteConnectionManager>,
}

impl Database {
    pub fn new(path: &str) -> Result<Self> {
        let manager = SqliteConnectionManager::file(path);
        let pool = Pool::builder()
            .max_size(10)
            .min_idle(Some(2))
            .build(manager)?;
        
        let db = Self { pool };
        db.init_tables()?;
        Ok(db)
    }

    fn conn(&self) -> Result<PooledConnection<SqliteConnectionManager>> {
        Ok(self.pool.get()?)
    }

    fn init_tables(&self) -> Result<()> {
        let conn = self.conn()?;
        
        // 启用 WAL 模式提升并发性能
        conn.execute_batch("
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA cache_size = 10000;
            PRAGMA temp_store = MEMORY;
        ")?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS proxy_rules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                source TEXT NOT NULL,
                target TEXT NOT NULL,
                timeout_secs INTEGER DEFAULT 30,
                enabled INTEGER DEFAULT 1,
                created_at TEXT DEFAULT (datetime('now', 'localtime')),
                updated_at TEXT DEFAULT (datetime('now', 'localtime'))
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS system_config (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                key TEXT UNIQUE NOT NULL,
                value TEXT NOT NULL
            )",
            [],
        )?;

        // 创建索引
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_rules_enabled ON proxy_rules(enabled)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_config_key ON system_config(key)",
            [],
        )?;

        conn.execute(
            "INSERT OR IGNORE INTO system_config (key, value) VALUES ('direct_proxy_path', 'proxy')",
            [],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO system_config (key, value) VALUES ('proxy_port', '3000')",
            [],
        )?;

        Ok(())
    }

    pub fn get_all_rules(&self) -> Result<Vec<ProxyRule>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare_cached(
            "SELECT id, name, source, target, timeout_secs, enabled, created_at, updated_at 
             FROM proxy_rules ORDER BY id"
        )?;
        
        let rules = stmt.query_map([], |row| {
            Ok(ProxyRule {
                id: row.get(0)?,
                name: row.get(1)?,
                source: row.get(2)?,
                target: row.get(3)?,
                timeout_secs: row.get::<_, i64>(4)? as u64,
                enabled: row.get::<_, i64>(5)? == 1,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(rules)
    }

    pub fn get_enabled_rules(&self) -> Result<Vec<ProxyRule>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare_cached(
            "SELECT id, name, source, target, timeout_secs, enabled, created_at, updated_at 
             FROM proxy_rules WHERE enabled = 1 ORDER BY id"
        )?;
        
        let rules = stmt.query_map([], |row| {
            Ok(ProxyRule {
                id: row.get(0)?,
                name: row.get(1)?,
                source: row.get(2)?,
                target: row.get(3)?,
                timeout_secs: row.get::<_, i64>(4)? as u64,
                enabled: row.get::<_, i64>(5)? == 1,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(rules)
    }

    pub fn create_rule(&self, name: &str, source: &str, target: &str, timeout_secs: u64) -> Result<i64> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO proxy_rules (name, source, target, timeout_secs) VALUES (?1, ?2, ?3, ?4)",
            params![name, source, target, timeout_secs as i64],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_rule(&self, id: i64, name: &str, source: &str, target: &str, timeout_secs: u64, enabled: bool) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE proxy_rules SET name = ?1, source = ?2, target = ?3, timeout_secs = ?4, enabled = ?5, 
             updated_at = datetime('now', 'localtime') WHERE id = ?6",
            params![name, source, target, timeout_secs as i64, enabled as i64, id],
        )?;
        Ok(())
    }

    pub fn delete_rule(&self, id: i64) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM proxy_rules WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn toggle_rule(&self, id: i64, enabled: bool) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE proxy_rules SET enabled = ?1, updated_at = datetime('now', 'localtime') WHERE id = ?2",
            params![enabled as i64, id],
        )?;
        Ok(())
    }

    pub fn get_config(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare_cached("SELECT value FROM system_config WHERE key = ?1")?;
        let result = stmt.query_row(params![key], |row| row.get(0)).ok();
        Ok(result)
    }

    pub fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO system_config (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_all_configs(&self) -> Result<Vec<SystemConfig>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare_cached("SELECT id, key, value FROM system_config")?;
        let configs = stmt.query_map([], |row| {
            Ok(SystemConfig {
                id: row.get(0)?,
                key: row.get(1)?,
                value: row.get(2)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(configs)
    }
}
