use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use chrono::{Utc, Duration};

use crate::AdminState;

/// Session 数据
#[derive(Clone)]
pub struct Session {
    pub username: String,
    pub expires_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub success: bool,
    pub token: Option<String>,
    pub message: Option<String>,
}

/// 认证状态 - 使用 DashMap 实现无锁并发
#[derive(Clone)]
pub struct AuthState {
    pub username: String,
    pub password: String,
    pub sessions: Arc<DashMap<String, Session>>,
}

impl AuthState {
    pub fn new(username: String, password: String) -> Self {
        Self {
            username,
            password,
            sessions: Arc::new(DashMap::new()),
        }
    }

    #[inline]
    pub fn validate(&self, username: &str, password: &str) -> bool {
        self.username == username && self.password == password
    }

    pub fn create_session(&self, username: &str) -> String {
        let token = generate_token();
        let session = Session {
            username: username.to_string(),
            expires_at: (Utc::now() + Duration::hours(24)).timestamp(),
        };
        self.sessions.insert(token.clone(), session);
        token
    }

    #[inline]
    pub fn validate_session(&self, token: &str) -> bool {
        self.sessions
            .get(token)
            .map(|s| s.expires_at > Utc::now().timestamp())
            .unwrap_or(false)
    }

    pub fn remove_session(&self, token: &str) {
        self.sessions.remove(token);
    }

    /// 清理过期 session
    pub fn cleanup_expired(&self) {
        let now = Utc::now().timestamp();
        self.sessions.retain(|_, s| s.expires_at > now);
    }
}

fn generate_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let rand = RandomState::new().build_hasher().finish();
    format!("{:x}{:x}", timestamp, rand)
}

/// 登录处理
pub async fn login_handler(
    State(state): State<AdminState>,
    Json(req): Json<LoginRequest>,
) -> Json<LoginResponse> {
    if state.auth.validate(&req.username, &req.password) {
        let token = state.auth.create_session(&req.username);
        Json(LoginResponse {
            success: true,
            token: Some(token),
            message: None,
        })
    } else {
        Json(LoginResponse {
            success: false,
            token: None,
            message: Some("用户名或密码错误".to_string()),
        })
    }
}

/// 登出处理
pub async fn logout_handler(
    State(state): State<AdminState>,
    req: Request<axum::body::Body>,
) -> impl IntoResponse {
    if let Some(token) = extract_token(&req) {
        state.auth.remove_session(&token);
    }
    Json(serde_json::json!({"success": true}))
}

/// 验证会话
pub async fn check_session_handler(
    State(state): State<AdminState>,
    req: Request<axum::body::Body>,
) -> impl IntoResponse {
    let valid = extract_token(&req)
        .map(|t| state.auth.validate_session(&t))
        .unwrap_or(false);
    Json(serde_json::json!({"valid": valid}))
}

/// 认证中间件
pub async fn auth_middleware(
    State(state): State<AdminState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let path = req.uri().path();
    
    // 白名单路径 - 只允许登录相关和静态资源
    if matches!(path, "/api/login" | "/api/session" | "/login" | "/favicon.ico")
        || path.starts_with("/static/")
    {
        return next.run(req).await;
    }

    // 验证 token
    if let Some(token) = extract_token(&req) {
        if state.auth.validate_session(&token) {
            return next.run(req).await;
        }
    }

    // 页面请求重定向到登录页，API 请求返回 401
    if path.starts_with("/api/") {
        (StatusCode::UNAUTHORIZED, "Unauthorized").into_response()
    } else {
        axum::response::Redirect::to("/login").into_response()
    }
}

#[inline]
fn extract_token<B>(req: &Request<B>) -> Option<String> {
    // Authorization header
    if let Some(auth) = req.headers().get("Authorization") {
        if let Ok(s) = auth.to_str() {
            if let Some(token) = s.strip_prefix("Bearer ") {
                return Some(token.to_string());
            }
        }
    }
    
    // Cookie
    if let Some(cookie) = req.headers().get("Cookie") {
        if let Ok(s) = cookie.to_str() {
            for part in s.split(';') {
                if let Some(token) = part.trim().strip_prefix("token=") {
                    return Some(token.to_string());
                }
            }
        }
    }
    
    None
}
