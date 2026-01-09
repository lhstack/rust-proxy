use arc_swap::ArcSwap;
use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    response::Response,
};
use futures::StreamExt;
use regex::Regex;
use reqwest::Client;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use crate::db::ProxyRule;

/// 编译后的代理规则
#[derive(Debug, Clone)]
pub struct CompiledProxyRule {
    pub source_pattern: Regex,
    pub target_template: String,
    pub param_names: Vec<String>,
    pub timeout: Duration,
}

impl CompiledProxyRule {
    pub fn from_db_rule(rule: &ProxyRule) -> Result<Self, regex::Error> {
        let (pattern, param_names) = Self::compile_pattern(&rule.source);
        let regex = Regex::new(&pattern)?;

        Ok(Self {
            source_pattern: regex,
            target_template: rule.target.clone(),
            param_names,
            timeout: Duration::from_secs(rule.timeout_secs),
        })
    }

    fn compile_pattern(source: &str) -> (String, Vec<String>) {
        let mut pattern = String::from("^");
        let mut param_names = Vec::new();
        let mut last_end = 0;

        let param_regex = Regex::new(r"\{(\*?)(\w+)\}").unwrap();

        for cap in param_regex.captures_iter(source) {
            let full_match = cap.get(0).unwrap();
            let is_wildcard = !cap.get(1).unwrap().as_str().is_empty();
            let name = cap.get(2).unwrap().as_str();

            pattern.push_str(&regex::escape(&source[last_end..full_match.start()]));

            if is_wildcard {
                pattern.push_str("(.+)");
            } else {
                pattern.push_str("([^/]+)");
            }

            param_names.push(format!(
                "{{{}{}}}",
                if is_wildcard { "*" } else { "" },
                name
            ));
            last_end = full_match.end();
        }

        pattern.push_str(&regex::escape(&source[last_end..]));
        pattern.push_str("(?:\\?.*)?$");

        (pattern, param_names)
    }

    #[inline]
    pub fn match_and_build_target(&self, path: &str) -> Option<String> {
        self.source_pattern.captures(path).map(|caps| {
            let mut target = self.target_template.clone();
            for (i, param_name) in self.param_names.iter().enumerate() {
                if let Some(value) = caps.get(i + 1) {
                    target = target.replace(param_name, value.as_str());
                }
            }
            target
        })
    }
}

/// 代理服务状态 - 使用 ArcSwap 实现无锁读取
#[derive(Clone)]
pub struct ProxyState {
    pub client: Client,
    pub rules: Arc<ArcSwap<Vec<CompiledProxyRule>>>,
    pub direct_proxy_path: Arc<ArcSwap<String>>,
    pub default_timeout: Duration,
}

/// 规则代理处理器 - 统一处理直接代理和规则代理，支持动态路径
pub async fn rule_proxy_handler(
    State(state): State<ProxyState>,
    ConnectInfo(client_addr): ConnectInfo<SocketAddr>,
    req: Request,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();
    let query = req.uri().query();
    let client_ip = client_addr.ip().to_string();

    // 无锁读取直接代理路径
    let direct_path = state.direct_proxy_path.load();
    let direct_path_str = direct_path.as_str();
    let direct_prefix = format!("/{}/", direct_path_str);

    tracing::debug!("Request path: {}, direct_prefix: {}", path, direct_prefix);

    // 检查是否是直接代理请求: /{path}/http://... 或 /{path}/https://...
    if path.starts_with(&direct_prefix) {
        let target_url = &path[direct_prefix.len()..];
        tracing::debug!("Checking direct proxy, target_url: {}", target_url);

        if target_url.starts_with("http://") || target_url.starts_with("https://") {
            let final_url = match query {
                Some(q) => format!("{}?{}", target_url, q),
                None => target_url.to_string(),
            };

            tracing::info!(method = %req.method(), target = %final_url, client_ip = %client_ip, "Direct proxy");
            return forward_request_streaming(
                req,
                &final_url,
                &state.client,
                state.default_timeout,
                &client_ip,
            )
            .await;
        }
    }

    // 无锁读取规则，查找匹配的规则
    let rules = state.rules.load();
    for rule in rules.iter() {
        if let Some(mut target_url) = rule.match_and_build_target(path) {
            if let Some(q) = query {
                target_url.push('?');
                target_url.push_str(q);
            }

            tracing::info!(method = %req.method(), source = %path, target = %target_url, client_ip = %client_ip, "Rule proxy");
            return forward_request_streaming(
                req,
                &target_url,
                &state.client,
                rule.timeout,
                &client_ip,
            )
            .await;
        }
    }

    tracing::warn!("No matching rule for path: {}", path);
    Err(StatusCode::NOT_FOUND)
}

/// 流式转发请求 - 避免大响应体占用内存
async fn forward_request_streaming(
    req: Request,
    target_url: &str,
    client: &Client,
    timeout: Duration,
    client_ip: &str,
) -> Result<Response, StatusCode> {
    let method = req.method().clone();
    let headers = req.headers().clone();

    // 流式读取请求体
    let body_stream = req.into_body();
    let body_bytes = axum::body::to_bytes(body_stream, 100 * 1024 * 1024) // 100MB 限制
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    // 构建请求
    let mut forward_req = client
        .request(convert_method(&method), target_url)
        .timeout(timeout);

    // 复制请求头
    for (name, value) in headers.iter() {
        if !is_hop_by_hop_header(name.as_str()) {
            if let (Ok(n), Ok(v)) = (
                reqwest::header::HeaderName::from_bytes(name.as_ref()),
                reqwest::header::HeaderValue::from_bytes(value.as_bytes()),
            ) {
                forward_req = forward_req.header(n, v);
            }
        }
    }

    // 添加代理相关头，传递真实客户端 IP
    // X-Forwarded-For: 追加客户端 IP 到现有链
    let xff = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|existing| format!("{}, {}", existing, client_ip))
        .unwrap_or_else(|| client_ip.to_string());
    forward_req = forward_req.header("X-Forwarded-For", &xff);

    // X-Real-IP: 原始客户端 IP（如果还没设置）
    if !headers.contains_key("x-real-ip") {
        forward_req = forward_req.header("X-Real-IP", client_ip);
    }

    // X-Forwarded-Proto: 协议
    if !headers.contains_key("x-forwarded-proto") {
        let proto = if target_url.starts_with("https://") {
            "https"
        } else {
            "http"
        };
        forward_req = forward_req.header("X-Forwarded-Proto", proto);
    }

    if !body_bytes.is_empty() {
        forward_req = forward_req.body(body_bytes.to_vec());
    }

    // 发送请求
    let response = forward_req.send().await.map_err(|e| {
        tracing::error!("Proxy error: {}", e);
        if e.is_timeout() {
            StatusCode::GATEWAY_TIMEOUT
        } else {
            StatusCode::BAD_GATEWAY
        }
    })?;

    let status = StatusCode::from_u16(response.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    // 复制响应头
    let mut response_headers = HeaderMap::new();
    for (name, value) in response.headers().iter() {
        if !is_hop_by_hop_header(name.as_str()) {
            if let (Ok(n), Ok(v)) = (
                HeaderName::from_bytes(name.as_ref()),
                HeaderValue::from_bytes(value.as_bytes()),
            ) {
                response_headers.insert(n, v);
            }
        }
    }

    // 流式响应体
    let body_stream = response
        .bytes_stream()
        .map(|result| result.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)));

    let body = Body::from_stream(body_stream);

    let mut resp = Response::new(body);
    *resp.status_mut() = status;
    *resp.headers_mut() = response_headers;

    Ok(resp)
}

#[inline]
fn convert_method(method: &Method) -> reqwest::Method {
    match *method {
        Method::GET => reqwest::Method::GET,
        Method::POST => reqwest::Method::POST,
        Method::PUT => reqwest::Method::PUT,
        Method::DELETE => reqwest::Method::DELETE,
        Method::HEAD => reqwest::Method::HEAD,
        Method::OPTIONS => reqwest::Method::OPTIONS,
        Method::PATCH => reqwest::Method::PATCH,
        Method::TRACE => reqwest::Method::TRACE,
        Method::CONNECT => reqwest::Method::CONNECT,
        _ => reqwest::Method::GET,
    }
}

#[inline]
fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
            | "host"
    )
}
