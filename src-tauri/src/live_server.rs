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
