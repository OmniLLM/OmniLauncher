use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::RwLock,
};

#[derive(Debug)]
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

type RouteFuture = Pin<Box<dyn Future<Output = LiveResponse> + Send>>;
type RouteHandler = Arc<dyn Fn() -> RouteFuture + Send + Sync>;
type QueryRouteHandler = Arc<dyn Fn(String) -> RouteFuture + Send + Sync>;

#[derive(Clone, Default)]
pub struct LiveServer {
    routes: Arc<RwLock<HashMap<String, RouteHandler>>>,
    query_routes: Arc<RwLock<HashMap<String, QueryRouteHandler>>>,
}

impl LiveServer {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register_route<F, Fut>(&self, path: impl Into<String>, handler: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = LiveResponse> + Send + 'static,
    {
        let wrapped: RouteHandler = Arc::new(move || {
            let fut = handler();
            Box::pin(fut) as RouteFuture
        });
        self.routes
            .write()
            .await
            .insert(normalize_path(&path.into()), wrapped);
    }

    /// Register a route whose handler receives the raw URL query string
    /// (without leading `?`). Empty when the request had no query.
    pub async fn register_route_with_query<F, Fut>(&self, path: impl Into<String>, handler: F)
    where
        F: Fn(String) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = LiveResponse> + Send + 'static,
    {
        let wrapped: QueryRouteHandler = Arc::new(move |q| {
            let fut = handler(q);
            Box::pin(fut) as RouteFuture
        });
        self.query_routes
            .write()
            .await
            .insert(normalize_path(&path.into()), wrapped);
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
            let query_routes = self.query_routes.clone();
            tokio::spawn(async move {
                let request = match read_http_request(&mut stream).await {
                    Ok(r) => r,
                    Err(resp) => {
                        let bytes = encode_response(resp);
                        if let Err(error) = stream.write_all(&bytes).await {
                            log::debug!("live server write error to {}: {}", addr, error);
                        }
                        let _ = stream.shutdown().await;
                        return;
                    }
                };

                let first_line = request.lines().next().unwrap_or_default();
                let request_path = first_line.split_whitespace().nth(1).unwrap_or("/");
                let (path, query) = split_path_query(request_path);

                let response = if path == "/health" {
                    LiveResponse::json("{\"ok\":true}".to_string())
                } else if let Some(handler) = query_routes.read().await.get(&path).cloned() {
                    handler(query).await
                } else {
                    match routes.read().await.get(&path).cloned() {
                        Some(handler) => handler().await,
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

/// Read a complete HTTP request from `stream`, returning it as a `String`.
///
/// * Reads until `\r\n\r\n` with a 64 KiB header cap and a 30-second timeout.
/// * Parses `Content-Length` and reads exactly that many additional body bytes,
///   rejecting payloads larger than 16 MiB.
async fn read_http_request(stream: &mut tokio::net::TcpStream) -> Result<String, LiveResponse> {
    const HEADER_CAP: usize = 64 * 1024;
    const BODY_CAP: usize = 16 * 1024 * 1024;
    const TIMEOUT_SECS: u64 = 30;

    let result = tokio::time::timeout(std::time::Duration::from_secs(TIMEOUT_SECS), async {
        let mut raw: Vec<u8> = Vec::with_capacity(4096);
        let mut tmp = [0u8; 4096];
        let header_end = loop {
            let n = stream
                .read(&mut tmp)
                .await
                .map_err(|_| LiveResponse::text("400 Bad Request", "read error".to_string()))?;
            if n == 0 {
                return Err(LiveResponse::text(
                    "400 Bad Request",
                    "connection closed".to_string(),
                ));
            }
            raw.extend_from_slice(&tmp[..n]);
            if raw.len() > HEADER_CAP {
                return Err(LiveResponse::text(
                    "431 Request Header Fields Too Large",
                    "header too large".to_string(),
                ));
            }
            if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
        };

        let header_str = String::from_utf8_lossy(&raw[..header_end]);
        let content_length: Option<usize> = header_str
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
            .and_then(|l| l["content-length:".len()..].trim().parse().ok());

        if let Some(cl) = content_length {
            if cl > BODY_CAP {
                return Err(LiveResponse::text(
                    "413 Payload Too Large",
                    "request body too large".to_string(),
                ));
            }
            let already = raw.len() - header_end;
            let remaining = cl.saturating_sub(already);
            if remaining > 0 {
                let old_len = raw.len();
                raw.resize(old_len + remaining, 0);
                stream.read_exact(&mut raw[old_len..]).await.map_err(|_| {
                    LiveResponse::text("400 Bad Request", "body read error".to_string())
                })?;
            }
        }

        Ok(String::from_utf8_lossy(&raw).into_owned())
    })
    .await;

    match result {
        Ok(inner) => inner,
        Err(_elapsed) => Err(LiveResponse::text(
            "408 Request Timeout",
            "request timed out".to_string(),
        )),
    }
}

/// Split a request target like `/foo/bar?x=1&y=2` into (`/foo/bar`, `x=1&y=2`).
fn split_path_query(target: &str) -> (String, String) {
    match target.split_once('?') {
        Some((p, q)) => (normalize_path(p), q.to_string()),
        None => (normalize_path(target), String::new()),
    }
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
        response.body.len()
    );
    [header.into_bytes(), response.body.into_bytes()].concat()
}
