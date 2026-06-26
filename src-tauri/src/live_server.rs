use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use tokio::{net::TcpListener, sync::RwLock};

use crate::http_util::{
    self, encode_response, normalize_path, read_http_request, split_path_query, HttpLimits,
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
            let (mut stream, _addr) = match listener.accept().await {
                Ok(parts) => parts,
                Err(error) => {
                    log::warn!("live server accept error: {}", error);
                    continue;
                }
            };

            let routes = self.routes.clone();
            let query_routes = self.query_routes.clone();
            tokio::spawn(async move {
                let request = match read_http_request(&mut stream, HttpLimits::DEFAULT).await {
                    Ok(r) => r,
                    Err(resp) => {
                        let bytes = encode_response(resp, None);
                        http_util::write_and_close(&mut stream, &bytes).await;
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

                let bytes = encode_response(response, None);
                http_util::write_and_close(&mut stream, &bytes).await;
            });
        }
    }
}
