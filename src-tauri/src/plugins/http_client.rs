use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

/// HTTP API client tool - inspired by hermes web_extract and openclaw browser tools
pub struct HttpClientPlugin;

#[async_trait]
impl Plugin for HttpClientPlugin {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Make HTTP requests (GET, POST, PUT, DELETE)"
    }

    fn keyword(&self) -> Option<&str> {
        None
    }

    async fn query(&self, _q: &Query) -> Vec<QueryResult> {
        vec![]
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "http_request",
                "description": "Make an HTTP request to an API endpoint. Supports GET, POST, PUT, DELETE with JSON bodies and custom headers.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "method": { "type": "string", "enum": ["GET", "POST", "PUT", "DELETE", "PATCH"], "description": "HTTP method" },
                        "url": { "type": "string", "description": "URL to request" },
                        "body": { "type": "string", "description": "Request body (JSON string for POST/PUT)" },
                        "headers": { "type": "object", "description": "Custom headers as key-value pairs" }
                    },
                    "required": ["method", "url"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let method = args["method"].as_str().unwrap_or("GET");
        let url = args["url"].as_str().unwrap_or("");
        let body = args["body"].as_str();
        let headers = args["headers"].as_object();

        if url.is_empty() {
            return "Error: no URL provided".to_string();
        }

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
        {
            Ok(c) => c,
            Err(e) => return format!("Error creating client: {}", e),
        };

        let mut req = match method.to_uppercase().as_str() {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            "PATCH" => client.patch(url),
            _ => return format!("Unsupported method: {}", method),
        };

        // Add headers
        if let Some(hdrs) = headers {
            for (key, val) in hdrs {
                if let Some(v) = val.as_str() {
                    req = req.header(key.as_str(), v);
                }
            }
        }

        // Add body
        if let Some(b) = body {
            req = req
                .header("Content-Type", "application/json")
                .body(b.to_string());
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                let headers_str: String = resp
                    .headers()
                    .iter()
                    .take(10)
                    .map(|(k, v)| format!("{}: {}", k, v.to_str().unwrap_or("")))
                    .collect::<Vec<_>>()
                    .join("\n");

                match resp.text().await {
                    Ok(text) => {
                        let body_str = if text.len() > 6000 {
                            format!("{}\n... (truncated)", &text[..6000])
                        } else {
                            text
                        };
                        format!(
                            "Status: {}\n\nHeaders:\n{}\n\nBody:\n{}",
                            status, headers_str, body_str
                        )
                    }
                    Err(e) => format!("Status: {}\nError reading body: {}", status, e),
                }
            }
            Err(e) => format!("Request error: {}", e),
        }
    }
}
