use std::{collections::HashMap, sync::Arc};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::RwLock,
};

pub struct LiveResponse {
    pub status: &'static str,
    pub content_type: &'static str,
    pub body: String,
}

impl LiveResponse {
    pub fn html(body: String) -> Self {
        Self {
            status: "200 OK",
            content_type: "text/html; charset=utf-8",
            body,
        }
    }

    pub fn json(body: String) -> Self {
        Self {
            status: "200 OK",
            content_type: "application/json; charset=utf-8",
            body,
        }
    }

    pub fn text(status: &'static str, body: String) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body,
        }
    }
}

type RouteHandler = Arc<dyn Fn() -> LiveResponse + Send + Sync>;

#[derive(Clone, Default)]
pub struct LiveServer {
    routes: Arc<RwLock<HashMap<String, RouteHandler>>>,
}

impl LiveServer {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register_route<F>(&self, path: impl Into<String>, handler: F)
    where
        F: Fn() -> LiveResponse + Send + Sync + 'static,
    {
        self.routes
            .write()
            .await
            .insert(normalize_path(&path.into()), Arc::new(handler));
    }

    pub fn url(&self, port: u16, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", port, normalize_path(path))
    }

    pub async fn serve(self, port: u16) {
        let listener = match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => listener,
            Err(error) => {
                log::error!("failed to bind live server on port {}: {}", port, error);
                return;
            }
        };

        log::info!("live server listening on http://127.0.0.1:{}", port);

        loop {
            let (mut stream, addr) = match listener.accept().await {
                Ok(parts) => parts,
                Err(error) => {
                    log::warn!("live server accept error: {}", error);
                    continue;
                }
            };

            let routes = self.routes.clone();
            tokio::spawn(async move {
                let mut buf = [0_u8; 4096];
                let read_len = match stream.read(&mut buf).await {
                    Ok(size) => size,
                    Err(error) => {
                        log::debug!("live server read error from {}: {}", addr, error);
                        return;
                    }
                };

                if read_len == 0 {
                    return;
                }

                let request = String::from_utf8_lossy(&buf[..read_len]);
                let first_line = request.lines().next().unwrap_or_default();
                let request_path = first_line.split_whitespace().nth(1).unwrap_or("/");
                let path = normalize_request_target(request_path);

                let response = if path == "/health" {
                    LiveResponse::json("{\"ok\":true}".to_string())
                } else {
                    match routes.read().await.get(&path).cloned() {
                        Some(handler) => handler(),
                        None => LiveResponse::text("404 Not Found", "Not Found".to_string()),
                    }
                };

                let bytes = encode_response(response);
                if let Err(error) = stream.write_all(&bytes).await {
                    log::debug!("live server write error to {}: {}", addr, error);
                }
                let _ = stream.shutdown().await;
            });
        }
    }
}

fn normalize_request_target(target: &str) -> String {
    let path_only = target.split('?').next().unwrap_or("/");
    normalize_path(path_only)
}

fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        "/".to_string()
    } else {
        format!("/{}", trimmed.trim_matches('/'))
    }
}

fn encode_response(response: LiveResponse) -> Vec<u8> {
    let header = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nCache-Control: no-store, no-cache, must-revalidate\r\nPragma: no-cache\r\nExpires: 0\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.as_bytes().len()
    );
    [header.into_bytes(), response.body.into_bytes()].concat()
}
