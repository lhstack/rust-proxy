mod api;
mod auth;
mod config;
mod db;
mod logger;
mod proxy;
mod static_files;

use arc_swap::ArcSwap;
use axum::{
    middleware,
    routing::{any, delete, get, post, put},
    Router,
};
use reqwest::Client;
use std::sync::atomic::AtomicU16;
use std::sync::Arc;
use std::time::Duration;
use tower_http::{compression::CompressionLayer, trace::TraceLayer};
use tracing_subscriber::{fmt::time::FormatTime, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::auth::AuthState;
use crate::config::Config;
use crate::db::Database;
use crate::logger::{start_cleanup_task, RollingFileWriter};
use crate::proxy::{CompiledProxyRule, ProxyState, rule_proxy_handler};

struct CustomTimer;

impl FormatTime for CustomTimer {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))
    }
}

/// 管理界面状态
#[derive(Clone)]
pub struct AdminState {
    pub db: Database,
    pub rules: Arc<ArcSwap<Vec<CompiledProxyRule>>>,
    pub direct_proxy_path: Arc<ArcSwap<String>>,
    pub proxy_port: Arc<AtomicU16>,
    pub auth: AuthState,
}

impl AdminState {
    pub fn reload_rules(&self) -> anyhow::Result<()> {
        let db_rules = self.db.get_enabled_rules()?;
        let compiled: Vec<CompiledProxyRule> = db_rules
            .iter()
            .filter_map(|rule| {
                match CompiledProxyRule::from_db_rule(rule) {
                    Ok(compiled) => {
                        tracing::info!(name = %rule.name, source = %rule.source, "Loaded rule");
                        Some(compiled)
                    }
                    Err(e) => {
                        tracing::error!(source = %rule.source, error = %e, "Failed to compile rule");
                        None
                    }
                }
            })
            .collect();

        self.rules.store(Arc::new(compiled));
        tracing::info!("Reloaded {} proxy rules", self.rules.load().len());
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load("config.yaml").expect("Failed to load config.yaml");

    // 日志初始化
    let file_writer = RollingFileWriter::new(&config.logging.directory, config.logging.max_size_bytes)?;

    tracing_subscriber::registry()
        .with(EnvFilter::new("info,hyper=warn,reqwest=warn"))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file_writer)
                .with_ansi(false)
                .with_timer(CustomTimer)
                .with_target(false)
                .with_file(true)
                .with_line_number(true),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .with_timer(CustomTimer)
                .with_target(false),
        )
        .init();

    tracing::info!("Starting proxy server...");

    start_cleanup_task(config.logging.directory.clone(), config.logging.retention_days);

    // 数据库连接池
    let db = Database::new(&config.database.path)?;
    tracing::info!("Database initialized: {}", config.database.path);

    let direct_proxy_path = db.get_config("direct_proxy_path")?.unwrap_or_else(|| "proxy".to_string());

    // 高性能 HTTP 客户端
    let client = Client::builder()
        .pool_max_idle_per_host(200)
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(60))
        .tcp_nodelay(true)
        .http2_keep_alive_interval(Duration::from_secs(30))
        .http2_keep_alive_timeout(Duration::from_secs(10))
        .gzip(true)
        .brotli(true)
        .deflate(true)
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()?;

    // 使用 ArcSwap 实现无锁读取
    let rules = Arc::new(ArcSwap::from_pointee(Vec::new()));
    let direct_path = Arc::new(ArcSwap::from_pointee(direct_proxy_path.clone()));
    let proxy_port = Arc::new(AtomicU16::new(config.proxy.port));

    let auth_state = AuthState::new(config.auth.username.clone(), config.auth.password.clone());

    let admin_state = AdminState {
        db: db.clone(),
        rules: rules.clone(),
        direct_proxy_path: direct_path.clone(),
        proxy_port: proxy_port.clone(),
        auth: auth_state.clone(),
    };

    let proxy_state = ProxyState {
        client,
        rules: rules.clone(),
        direct_proxy_path: direct_path.clone(),
        default_timeout: Duration::from_secs(config.default_timeout_secs),
    };

    // 加载规则
    admin_state.reload_rules()?;

    // 启动 session 清理任务
    let auth_cleanup = auth_state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            auth_cleanup.cleanup_expired();
        }
    });

    // 管理界面路由 (带压缩)
    let admin_app = Router::new()
        .route("/", get(static_files::index_handler))
        .route("/login", get(static_files::login_page))
        .route("/api/login", post(auth::login_handler))
        .route("/api/logout", post(auth::logout_handler))
        .route("/api/session", get(auth::check_session_handler))
        .route("/api/rules", get(api::list_rules))
        .route("/api/rules", post(api::create_rule))
        .route("/api/rules/:id", put(api::update_rule))
        .route("/api/rules/:id", delete(api::delete_rule))
        .route("/api/rules/:id/toggle", post(api::toggle_rule))
        .route("/api/configs", get(api::get_configs))
        .route("/api/configs/:key", put(api::update_config))
        .route("/api/status", get(api::get_proxy_status))
        .route("/static/*path", get(static_files::serve_static))
        .layer(middleware::from_fn_with_state(admin_state.clone(), auth::auth_middleware))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(admin_state);

    // 代理服务路由 - 使用 fallback 处理所有请求，支持动态路径
    let proxy_app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .fallback(any(rule_proxy_handler))
        .with_state(proxy_state);

    let admin_addr = format!("{}:{}", config.admin.host, config.admin.port);
    let proxy_addr = format!("{}:{}", config.proxy.host, config.proxy.port);

    tracing::info!("Admin: http://{}", admin_addr);
    tracing::info!("Proxy: http://{}", proxy_addr);
    tracing::info!("Direct proxy path from DB: '{}', use: /{}/https://...", direct_proxy_path, direct_proxy_path);

    let admin_listener = tokio::net::TcpListener::bind(&admin_addr).await?;
    let proxy_listener = tokio::net::TcpListener::bind(&proxy_addr).await?;

    // 需要使用 into_make_service_with_connect_info 来获取客户端 IP
    use std::net::SocketAddr;

    tokio::select! {
        r = axum::serve(admin_listener, admin_app) => { r?; }
        r = axum::serve(proxy_listener, proxy_app.into_make_service_with_connect_info::<SocketAddr>()) => { r?; }
    }

    Ok(())
}
