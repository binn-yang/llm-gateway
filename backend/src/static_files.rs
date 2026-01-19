//! Static file serving for the dashboard
//!
//! Serves embedded frontend files using rust-embed

use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../frontend/dist/"]
struct Assets;

/// Serve static files from the embedded frontend
pub async fn serve_static(uri: Uri) -> impl IntoResponse {
    let path = uri.path();

    // Try to get the file from embedded assets
    let path_without_prefix = path.trim_start_matches('/');

    // Serve index.html for SPA routes (non-file paths)
    if !path_without_prefix.contains('.') {
        if let Some(content) = Assets::get("index.html") {
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/html")
                .body(Body::from(content.data.to_vec()))
                .unwrap();
        }
    }

    // Try to serve the specific file
    if let Some(content) = Assets::get(path_without_prefix.trim_start_matches('/')) {
        let mime = mime_guess::from_path(path_without_prefix)
            .first_or_octet_stream()
            .to_string();

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .body(Body::from(content.data.to_vec()))
            .unwrap();
    }

    // Fallback to index.html for SPA routing
    if let Some(content) = Assets::get("index.html") {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html")
            .body(Body::from(content.data.to_vec()))
            .unwrap();
    }

    // Nothing found
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("Not Found"))
        .unwrap()
}

/// Serve the dashboard index.html
pub async fn serve_index() -> impl IntoResponse {
    if let Some(content) = Assets::get("index.html") {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html")
            .body(Body::from(content.data.to_vec()))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::SERVICE_UNAVAILABLE)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(
            "Dashboard not built. Run `cd frontend && npm install && npm run build`",
        ))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_files_compile() {
        // This test verifies that the module compiles
        // The actual functionality will be tested after frontend is built
    }
}
