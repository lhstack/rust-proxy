use axum::{
    extract::State,
    http::StatusCode,
    Json,
    extract::Path,
};
use serde::{Deserialize, Serialize};

use crate::AdminState;

#[derive(Debug, Deserialize)]
pub struct CreateRuleRequest {
    pub name: String,
    pub source: String,
    pub target: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRuleRequest {
    pub name: String,
    pub source: String,
    pub target: String,
    pub timeout_secs: u64,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct ToggleRuleRequest {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
}

fn default_timeout() -> u64 { 30 }

impl<T> ApiResponse<T> {
    #[inline]
    pub fn ok(data: T) -> Self {
        Self { success: true, data: Some(data), message: None }
    }
}

pub async fn list_rules(
    State(state): State<AdminState>,
) -> Result<Json<ApiResponse<Vec<crate::db::ProxyRule>>>, StatusCode> {
    state.db.get_all_rules()
        .map(|rules| Json(ApiResponse::ok(rules)))
        .map_err(|e| {
            tracing::error!("Failed to list rules: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn create_rule(
    State(state): State<AdminState>,
    Json(req): Json<CreateRuleRequest>,
) -> Result<Json<ApiResponse<i64>>, StatusCode> {
    match state.db.create_rule(&req.name, &req.source, &req.target, req.timeout_secs) {
        Ok(id) => {
            let _ = state.reload_rules();
            Ok(Json(ApiResponse::ok(id)))
        }
        Err(e) => {
            tracing::error!("Failed to create rule: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_rule(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateRuleRequest>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    match state.db.update_rule(id, &req.name, &req.source, &req.target, req.timeout_secs, req.enabled) {
        Ok(_) => {
            let _ = state.reload_rules();
            Ok(Json(ApiResponse::ok(())))
        }
        Err(e) => {
            tracing::error!("Failed to update rule: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_rule(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    match state.db.delete_rule(id) {
        Ok(_) => {
            let _ = state.reload_rules();
            Ok(Json(ApiResponse::ok(())))
        }
        Err(e) => {
            tracing::error!("Failed to delete rule: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn toggle_rule(
    State(state): State<AdminState>,
    Path(id): Path<i64>,
    Json(req): Json<ToggleRuleRequest>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    match state.db.toggle_rule(id, req.enabled) {
        Ok(_) => {
            let _ = state.reload_rules();
            Ok(Json(ApiResponse::ok(())))
        }
        Err(e) => {
            tracing::error!("Failed to toggle rule: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_configs(
    State(state): State<AdminState>,
) -> Result<Json<ApiResponse<Vec<crate::db::SystemConfig>>>, StatusCode> {
    state.db.get_all_configs()
        .map(|configs| Json(ApiResponse::ok(configs)))
        .map_err(|e| {
            tracing::error!("Failed to get configs: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn update_config(
    State(state): State<AdminState>,
    Path(key): Path<String>,
    Json(req): Json<UpdateConfigRequest>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    tracing::info!("Updating config: {} = {}", key, req.value);
    match state.db.set_config(&key, &req.value) {
        Ok(_) => {
            if key == "direct_proxy_path" {
                let new_path = req.value.clone();
                state.direct_proxy_path.store(std::sync::Arc::new(new_path.clone()));
                tracing::info!("Updated direct_proxy_path to: {}", new_path);
            }
            Ok(Json(ApiResponse::ok(())))
        }
        Err(e) => {
            tracing::error!("Failed to update config: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Serialize)]
pub struct ProxyStatus {
    pub running: bool,
    pub port: u16,
    pub rules_count: usize,
    pub direct_proxy_path: String,
}

pub async fn get_proxy_status(
    State(state): State<AdminState>,
) -> Result<Json<ApiResponse<ProxyStatus>>, StatusCode> {
    let rules = state.rules.load();
    let direct_path = state.direct_proxy_path.load();
    let port = state.proxy_port.load(std::sync::atomic::Ordering::Relaxed);
    
    Ok(Json(ApiResponse::ok(ProxyStatus {
        running: true,
        port,
        rules_count: rules.len(),
        direct_proxy_path: direct_path.as_ref().clone(),
    })))
}
