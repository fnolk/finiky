use crate::filesystem::FileSystem;
use axum::{
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use bytes::Bytes;
use std::sync::Arc;
use tracing as log;

pub struct HttpServer {
    port: u16,
    filesystem: Arc<dyn FileSystem>,
}

impl HttpServer {
    pub fn new(port: u16, filesystem: Box<dyn FileSystem>) -> Self {
        HttpServer {
            port,
            filesystem: Arc::from(filesystem),
        }
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let filesystem = Arc::clone(&self.filesystem);
        let app = Router::new()
            .route("/{*path}", get(Self::handle_request))
            .with_state(filesystem);

        use std::net::SocketAddr;
        let addr: SocketAddr = format!("0.0.0.0:{}", self.port).parse()?;
        log::info!("HTTP server listening on port {}", self.port);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }

    async fn handle_request(
        axum::extract::Path(path): axum::extract::Path<String>,
        axum::extract::State(filesystem): axum::extract::State<Arc<dyn FileSystem>>,
    ) -> Response {
        let path = path.as_str();

        log::debug!("HTTP request for: {}", path);

        if filesystem.exists(path).await {
            match filesystem.read_file(path).await {
                Ok(data) => {
                    let content_type = Self::guess_content_type(path);
                    let mut headers = HeaderMap::new();
                    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
                    headers.insert(
                        header::CONTENT_LENGTH,
                        HeaderValue::from_str(&data.len().to_string())
                            .unwrap_or(HeaderValue::from_static("0")),
                    );
                    (StatusCode::OK, headers, Bytes::from(data)).into_response()
                }
                Err(e) => {
                    log::error!("Error reading file {}: {}", path, e);
                    (StatusCode::INTERNAL_SERVER_ERROR, "Error reading file").into_response()
                }
            }
        } else {
            log::warn!("HTTP file not found: {}", path);
            (StatusCode::NOT_FOUND, "File not found").into_response()
        }
    }

    pub fn guess_content_type(path: &str) -> &'static str {
        let ext = path.rsplit('.').next().unwrap_or("");
        match ext.to_lowercase().as_str() {
            "html" | "htm" => "text/html",
            "css" => "text/css",
            "js" => "application/javascript",
            "json" => "application/json",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "svg" => "image/svg+xml",
            "ico" => "image/x-icon",
            "txt" => "text/plain",
            "iso" => "application/octet-stream",
            "img" => "application/octet-stream",
            "efi" => "application/octet-stream",
            "0" => "application/octet-stream",
            _ => "application/octet-stream",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_type_guessing() {
        assert_eq!(HttpServer::guess_content_type("test.html"), "text/html");
        assert_eq!(HttpServer::guess_content_type("test.txt"), "text/plain");
        assert_eq!(
            HttpServer::guess_content_type("boot.efi"),
            "application/octet-stream"
        );
        assert_eq!(HttpServer::guess_content_type("image.png"), "image/png");
    }
}
