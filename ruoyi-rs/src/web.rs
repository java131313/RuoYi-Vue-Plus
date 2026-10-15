use crate::db::Shared;
use axum::extract::State;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use std::path::Path;

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        "map" => "application/json",
        _ => "application/octet-stream",
    }
}

async fn serve_file(path: &Path, content_type: &str) -> Response {
    match tokio::fs::read(path).await {
        Ok(bytes) => (
            [(header::CONTENT_TYPE, content_type)],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Fallback handler: static files from web_dir + /uploads + SPA index.html.
pub async fn static_handler(State(st): State<Shared>, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("uploads/") {
        let rel = path.trim_start_matches("uploads/");
        if rel.contains("..") {
            return StatusCode::FORBIDDEN.into_response();
        }
        let full = Path::new(&st.cfg.upload_dir).join(rel);
        return serve_file(&full, content_type(rel)).await;
    }
    if path.is_empty() || path == "index.html" {
        let index = Path::new(&st.cfg.web_dir).join("index.html");
        if index.await_file().await {
            return serve_file(&index, "text/html; charset=utf-8").await;
        }
        // backend-only image (e.g. Sealos): no frontend bundled
        return (
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "ruoyi-rs API 运行中。此镜像未内置前端：前端部署见 GitHub Pages，或挂载 dist 到 WEB_DIR。",
        )
            .into_response();
    }
    let safe: String = path.chars().filter(|c| *c != '\\').collect();
    if safe.contains("..") {
        return StatusCode::FORBIDDEN.into_response();
    }
    let full = Path::new(&st.cfg.web_dir).join(&safe);
    if full.await_file().await {
        return serve_file(&full, content_type(&safe)).await;
    }
    // SPA fallback: unknown non-asset paths get index.html
    if !safe.contains('.') || safe.starts_with("assets/") {
        let index = Path::new(&st.cfg.web_dir).join("index.html");
        return serve_file(&index, "text/html; charset=utf-8").await;
    }
    StatusCode::NOT_FOUND.into_response()
}

#[allow(dead_code)]
trait IsFile {
    async fn await_file(&self) -> bool;
}

impl IsFile for Path {
    async fn await_file(&self) -> bool {
        tokio::fs::metadata(self).await.map(|m| m.is_file()).unwrap_or(false)
    }
}

/// Browser page navigations (Accept: text/html / Sec-Fetch-Dest: document) to
/// bare API-shaped paths are rewritten to index.html so deep links like
/// /system/user serve the SPA; axios calls (Accept: application/json, ...)
/// continue to the JSON API. /prod-api/* is exempt (API-only prefix).
pub async fn spa_document_guard(mut req: axum::http::Request<axum::body::Body>, next: axum::middleware::Next) -> Response {
    use axum::http::Method;
    if req.method() == Method::GET && !req.uri().path().starts_with("/prod-api") {
        let headers = req.headers();
        let accept = headers
            .get(axum::http::header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let dest_document = headers
            .get("sec-fetch-dest")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.eq_ignore_ascii_case("document"))
            .unwrap_or(false);
        let wants_html = accept.contains("text/html") && !accept.trim_start().starts_with("application/json");
        if dest_document || wants_html {
            let pq = req.uri().path().to_string();
            // real static files (with extensions) are not page navigations
            let has_extension = pq.rsplit('/').next().map(|seg| seg.contains('.')).unwrap_or(false);
            if !has_extension {
                *req.uri_mut() = axum::http::Uri::from_static("/index.html");
            }
        }
    }
    next.run(req).await
}
