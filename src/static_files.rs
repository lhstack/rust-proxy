use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{Html, IntoResponse, Response},
};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "static/"]
pub struct StaticAssets;

/// 静态资源服务 - 带缓存头
pub async fn serve_static(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/').trim_start_matches("static/");
    let path = if path.is_empty() { "index.html" } else { path };

    match StaticAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            
            // 静态资源缓存 1 天
            let cache_control = if path.ends_with(".html") {
                "no-cache"
            } else {
                "public, max-age=86400"
            };
            
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .header(header::CACHE_CONTROL, cache_control)
                .body(Body::from(content.data.to_vec()))
                .unwrap()
        }
        None => serve_index_or_404(),
    }
}

pub async fn index_handler() -> impl IntoResponse {
    match StaticAssets::get("index.html") {
        Some(content) => Html(String::from_utf8_lossy(&content.data).to_string()),
        None => Html("<h1>Admin panel not found</h1>".to_string()),
    }
}

pub async fn login_page() -> impl IntoResponse {
    match StaticAssets::get("login.html") {
        Some(content) => Html(String::from_utf8_lossy(&content.data).to_string()),
        None => Html("<h1>Login page not found</h1>".to_string()),
    }
}

fn serve_index_or_404() -> Response {
    if let Some(content) = StaticAssets::get("index.html") {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html")
            .body(Body::from(content.data.to_vec()))
            .unwrap()
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not Found"))
            .unwrap()
    }
}
