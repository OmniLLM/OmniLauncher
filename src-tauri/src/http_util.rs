//! Shared low-level HTTP plumbing used by the three internal servers
//! (`live_server`, the main `server`, and `a2a/server`).
//!
//! These helpers are extracted from byte-identical (or near-identical) copies
//! that previously lived in each server. Wire-format details that legitimately
//! differ between servers — CORS policy, auth header names, JSON-parse
//! logging — are parameterized via small policy structs / enums so callers
//! still control their externally-observable behavior.
//!
//! Body size limits (`HttpLimits`) happen to be identical across all three
//! servers today, but the struct is exposed so a future server can tighten
//! or loosen them without forking this module again.

use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::live_server::LiveResponse;

// ─── Policy types ──────────────────────────────────────────────────────────

/// Per-server CORS configuration for `encode_response`.
///
/// Pass `None` to `encode_response` on servers that should emit no CORS headers
/// (e.g. the embedded `live_server`, which only backs the local webview).
#[derive(Debug, Clone, Copy)]
pub struct CorsPolicy {
    pub methods: &'static str,
    pub headers: &'static str,
}

impl CorsPolicy {
    /// Main app server (`server.rs`): accepts DELETE and the custom token header.
    pub const APP: CorsPolicy = CorsPolicy {
        methods: "GET, POST, DELETE, OPTIONS",
        headers: "Content-Type, X-OmniLauncher-Token, Authorization",
    };

    /// A2A server: no DELETE, no custom token header.
    pub const A2A: CorsPolicy = CorsPolicy {
        methods: "GET, POST, OPTIONS",
        headers: "Content-Type, Authorization",
    };
}

/// Limits applied while reading an HTTP request from a TCP stream.
#[derive(Debug, Clone, Copy)]
pub struct HttpLimits {
    pub header_cap: usize,
    pub body_cap: usize,
    pub timeout_secs: u64,
}

impl HttpLimits {
    /// 64 KiB headers, 16 MiB body, 30-second total timeout.
    /// Matches the values previously duplicated in all three servers.
    pub const DEFAULT: HttpLimits = HttpLimits {
        header_cap: 64 * 1024,
        body_cap: 16 * 1024 * 1024,
        timeout_secs: 30,
    };
}

/// Which header(s) to consult when extracting an auth token from a request.
#[derive(Debug, Clone, Copy)]
pub enum AuthScheme {
    /// `Authorization: Bearer *** only (A2A protocol).
    Bearer,
    /// Try a custom header first, fall back to `Authorization: Bearer`.
    /// The custom header wins when both are present — preserves the
    /// original `extract_auth_header` behavior in `server.rs`.
    HeaderOrBearer { header: &'static str },
}

// ─── Request reading ───────────────────────────────────────────────────────

/// Read a full HTTP request (headers + body) from a TCP stream.
///
/// * Reads until `\r\n\r\n` with a `limits.header_cap` byte cap and a
///   `limits.timeout_secs`-second total timeout.
/// * Parses `Content-Length` and reads exactly that many additional body bytes,
///   rejecting payloads larger than `limits.body_cap`.
pub async fn read_http_request(
    stream: &mut TcpStream,
    limits: HttpLimits,
) -> Result<String, LiveResponse> {
    let result = tokio::time::timeout(std::time::Duration::from_secs(limits.timeout_secs), async {
        // ── Phase 1: read until \r\n\r\n ─────────────────────────────
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
            if raw.len() > limits.header_cap {
                return Err(LiveResponse::text(
                    "431 Request Header Fields Too Large",
                    "header too large".to_string(),
                ));
            }
            if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
        };

        // ── Phase 2: parse Content-Length ────────────────────────────
        let header_str = String::from_utf8_lossy(&raw[..header_end]);
        let content_length: Option<usize> = header_str
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
            .and_then(|l| l["content-length:".len()..].trim().parse().ok());

        // ── Phase 3: read body ────────────────────────────────────────
        if let Some(cl) = content_length {
            if cl > limits.body_cap {
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

// ─── Path / body parsing ───────────────────────────────────────────────────

/// Normalize an HTTP request-target path. Returns `"/"` for empty or `"/"`,
/// otherwise `"/<path>"` with leading/trailing slashes stripped.
pub fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        "/".to_string()
    } else {
        format!("/{}", trimmed.trim_matches('/'))
    }
}

/// Split a request target like `/foo/bar?x=1&y=2` into (`/foo/bar`, `x=1&y=2`).
/// The path component is normalized via [`normalize_path`].
pub fn split_path_query(target: &str) -> (String, String) {
    match target.split_once('?') {
        Some((path, query)) => (normalize_path(path), query.to_string()),
        None => (normalize_path(target), String::new()),
    }
}

/// Extract the body portion of an HTTP request: everything after `\r\n\r\n`.
/// Returns an empty string if the boundary is missing.
pub fn read_body(request: &str) -> String {
    match request.find("\r\n\r\n") {
        Some(pos) => request[pos + 4..].to_string(),
        None => String::new(),
    }
}

// ─── JSON helpers ──────────────────────────────────────────────────────────

/// Build a JSON response, falling back to a 500 with the serialization error.
pub fn json_response<T: Serialize>(value: &T) -> LiveResponse {
    match serde_json::to_string(value) {
        Ok(body) => LiveResponse::json(body),
        Err(error) => LiveResponse::text("500 Internal Server Error", error.to_string()),
    }
}

/// Parse a JSON request body, returning a 400 `LiveResponse` on failure.
///
/// When `log_failures` is true, the parse error is logged at warn level with
/// the body size. The main app server enables this to diagnose misbehaving
/// browser clients; A2A leaves it off because it returns structured A2A errors
/// at a higher layer.
pub fn parse_json<T: DeserializeOwned>(body: &str, log_failures: bool) -> Result<T, LiveResponse> {
    serde_json::from_str(body).map_err(|error| {
        if log_failures {
            log::warn!(
                "server JSON parse error: {} (body_bytes={})",
                error,
                body.len()
            );
        }
        LiveResponse::text("400 Bad Request", format!("Invalid JSON: {error}"))
    })
}

// ─── Response encoding ─────────────────────────────────────────────────────

/// Encode a [`LiveResponse`] into bytes ready to send over the wire.
///
/// When `cors` is `Some`, the response includes the standard CORS preflight
/// headers using the provided policy. Pass `None` to omit them entirely —
/// used by `live_server` for the local webview.
pub fn encode_response(response: LiveResponse, cors: Option<CorsPolicy>) -> Vec<u8> {
    use std::fmt::Write as _;

    let mut header = format!(
        "HTTP/1.1 {}\r\n\
         Content-Type: {}\r\n\
         Cache-Control: no-store, no-cache, must-revalidate\r\n\
         Pragma: no-cache\r\n\
         Expires: 0\r\n",
        response.status, response.content_type
    );
    if let Some(c) = cors {
        let _ = write!(
            header,
            "Access-Control-Allow-Origin: *\r\n\
             Access-Control-Allow-Methods: {}\r\n\
             Access-Control-Allow-Headers: {}\r\n",
            c.methods, c.headers
        );
    }
    let _ = write!(
        header,
        "Content-Length: {}\r\n\
         Connection: close\r\n\r\n",
        response.body.len()
    );
    [header.into_bytes(), response.body.into_bytes()].concat()
}

/// Write a byte buffer to the stream and close it cleanly. Used inside the
/// per-connection task to flush error responses produced by
/// [`read_http_request`] before tearing the connection down.
pub async fn write_and_close(stream: &mut TcpStream, bytes: &[u8]) {
    if let Err(error) = stream.write_all(bytes).await {
        log::debug!("http write error: {}", error);
    }
    let _ = stream.shutdown().await;
}

// ─── Auth ──────────────────────────────────────────────────────────────────

/// Extract an auth token from a request's headers per the given scheme.
///
/// For [`AuthScheme::HeaderOrBearer`], the custom header wins when both are
/// present — preserves the existing precedence in `server.rs`.
pub fn extract_auth(request: &str, scheme: AuthScheme) -> Option<&str> {
    let custom_header: Option<&'static str> = match scheme {
        AuthScheme::Bearer => None,
        AuthScheme::HeaderOrBearer { header } => Some(header),
    };

    let mut bearer: Option<&str> = None;
    for line in request.lines() {
        // Stop at the header/body boundary.
        if line.is_empty() {
            break;
        }
        let lower = line.to_ascii_lowercase();

        if let Some(h) = custom_header {
            // Build the lowercase prefix once per iteration. ASCII case folding
            // preserves byte length, so `prefix.len()` is also the correct
            // byte offset into the original (mixed-case) `line`.
            let h_lower = h.to_ascii_lowercase();
            if lower.starts_with(&h_lower) && lower[h_lower.len()..].starts_with(':') {
                let value_start = h_lower.len() + 1; // skip ':'
                return Some(line[value_start..].trim());
            }
        }

        if bearer.is_none() && lower.starts_with("authorization:") {
            let value = line["authorization:".len()..].trim();
            // Case-insensitive "Bearer " prefix check — RFC 6750 §2.1.
            if value.len() >= 7 && value[..7].eq_ignore_ascii_case("bearer ") {
                bearer = Some(value[7..].trim());
            }
        }
    }
    bearer
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── normalize_path / split_path_query ──────────────────────────────────

    #[test]
    fn normalize_path_handles_root_and_empty() {
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path(""), "/");
        assert_eq!(normalize_path("   "), "/");
    }

    #[test]
    fn normalize_path_strips_trailing_slashes() {
        assert_eq!(normalize_path("/api/health"), "/api/health");
        assert_eq!(normalize_path("/api/health/"), "/api/health");
        assert_eq!(normalize_path("api/health"), "/api/health");
    }

    #[test]
    fn split_path_query_no_query() {
        let (p, q) = split_path_query("/api/health");
        assert_eq!(p, "/api/health");
        assert_eq!(q, "");
    }

    #[test]
    fn split_path_query_with_query() {
        let (p, q) = split_path_query("/api/search?q=hello&limit=10");
        assert_eq!(p, "/api/search");
        assert_eq!(q, "q=hello&limit=10");
    }

    #[test]
    fn split_path_query_empty_query() {
        let (p, q) = split_path_query("/api/test?");
        assert_eq!(p, "/api/test");
        assert_eq!(q, "");
    }

    // ── read_body ──────────────────────────────────────────────────────────

    #[test]
    fn read_body_extracts_body_after_headers() {
        let raw = "POST /api/foo HTTP/1.1\r\nHost: x\r\n\r\n{\"a\":1}";
        assert_eq!(read_body(raw), "{\"a\":1}");
    }

    #[test]
    fn read_body_returns_empty_for_no_body() {
        let raw = "GET /api/foo HTTP/1.1\r\nHost: x\r\n\r\n";
        assert_eq!(read_body(raw), "");
    }

    #[test]
    fn read_body_returns_empty_for_malformed_request() {
        let raw = "broken request with no header boundary";
        assert_eq!(read_body(raw), "");
    }

    // ── encode_response ────────────────────────────────────────────────────

    #[test]
    fn encoded_response_no_cors_omits_access_control_headers() {
        let r = LiveResponse::text("204 No Content", String::new());
        let encoded = String::from_utf8(encode_response(r, None)).unwrap();
        assert!(encoded.starts_with("HTTP/1.1 204 No Content\r\n"));
        assert!(!encoded.contains("Access-Control-Allow-Origin"));
        assert!(encoded.contains("Content-Length: 0\r\n"));
        assert!(encoded.ends_with("\r\n\r\n"));
    }

    #[test]
    fn encoded_response_app_cors_includes_delete_and_custom_token() {
        let r = LiveResponse::text("204 No Content", String::new());
        let encoded = String::from_utf8(encode_response(r, Some(CorsPolicy::APP))).unwrap();
        assert!(encoded.contains("Access-Control-Allow-Origin: *\r\n"));
        assert!(encoded.contains("Access-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\n"));
        assert!(encoded.contains(
            "Access-Control-Allow-Headers: Content-Type, X-OmniLauncher-Token, Authorization\r\n"
        ));
    }

    #[test]
    fn encoded_response_a2a_cors_omits_delete_and_custom_token() {
        let r = LiveResponse::text("204 No Content", String::new());
        let encoded = String::from_utf8(encode_response(r, Some(CorsPolicy::A2A))).unwrap();
        assert!(encoded.contains("Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n"));
        assert!(encoded.contains("Access-Control-Allow-Headers: Content-Type, Authorization\r\n"));
        assert!(!encoded.contains("DELETE"));
        assert!(!encoded.contains("X-OmniLauncher-Token"));
    }

    #[test]
    fn encoded_response_includes_cache_control_and_connection_close() {
        let r = LiveResponse::json("{}".to_string());
        let encoded = String::from_utf8(encode_response(r, None)).unwrap();
        assert!(encoded.contains("Cache-Control: no-store, no-cache, must-revalidate\r\n"));
        assert!(encoded.contains("Pragma: no-cache\r\n"));
        assert!(encoded.contains("Expires: 0\r\n"));
        assert!(encoded.contains("Connection: close\r\n"));
    }

    #[test]
    fn encoded_response_correct_content_length_and_body() {
        let body = "Hello, World!";
        let r = LiveResponse::text("200 OK", body.to_string());
        let encoded = String::from_utf8(encode_response(r, None)).unwrap();
        assert!(encoded.contains(&format!("Content-Length: {}\r\n", body.len())));
        assert!(encoded.ends_with(body));
    }

    // ── parse_json / json_response ─────────────────────────────────────────

    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct Probe {
        query: String,
    }

    #[test]
    fn parse_json_valid() {
        let r: Result<Probe, _> = parse_json(r#"{"query":"hello"}"#, false);
        assert_eq!(
            r.unwrap(),
            Probe {
                query: "hello".to_string()
            }
        );
    }

    #[test]
    fn parse_json_invalid_returns_400() {
        let r: Result<Probe, _> = parse_json("not json", false);
        let err = r.unwrap_err();
        assert_eq!(err.status, "400 Bad Request");
    }

    #[test]
    fn parse_json_empty_returns_400() {
        let r: Result<Probe, _> = parse_json("", false);
        assert_eq!(r.unwrap_err().status, "400 Bad Request");
    }

    #[test]
    fn json_response_serializes_value() {
        let r = json_response(&serde_json::json!({"ok": true}));
        assert_eq!(r.status, "200 OK");
        assert_eq!(r.content_type, "application/json; charset=utf-8");
        assert!(r.body.contains(r#""ok":true"#));
    }

    // ── extract_auth ───────────────────────────────────────────────────────

    #[test]
    fn extract_auth_bearer_only_reads_authorization_bearer() {
        let req = "GET / HTTP/1.1\r\nAuthorization: Bearer my-token\r\n\r\n";
        assert_eq!(extract_auth(req, AuthScheme::Bearer), Some("my-token"));
    }

    #[test]
    fn extract_auth_bearer_only_ignores_custom_header() {
        let req = "GET / HTTP/1.1\r\nX-OmniLauncher-Token: secret\r\n\r\n";
        assert_eq!(extract_auth(req, AuthScheme::Bearer), None);
    }

    #[test]
    fn extract_auth_header_or_bearer_returns_none_when_no_auth() {
        let req = "GET / HTTP/1.1\r\nHost: x\r\n\r\n";
        let scheme = AuthScheme::HeaderOrBearer {
            header: "X-OmniLauncher-Token",
        };
        assert_eq!(extract_auth(req, scheme), None);
    }

    #[test]
    fn extract_auth_header_or_bearer_reads_custom_header() {
        let req = "GET / HTTP/1.1\r\nX-OmniLauncher-Token: secret-abc\r\n\r\n";
        let scheme = AuthScheme::HeaderOrBearer {
            header: "X-OmniLauncher-Token",
        };
        assert_eq!(extract_auth(req, scheme), Some("secret-abc"));
    }

    #[test]
    fn extract_auth_header_or_bearer_custom_header_case_insensitive() {
        let req = "GET / HTTP/1.1\r\nx-OMNILAUNCHER-token: secret-abc\r\n\r\n";
        let scheme = AuthScheme::HeaderOrBearer {
            header: "X-OmniLauncher-Token",
        };
        assert_eq!(extract_auth(req, scheme), Some("secret-abc"));
    }

    #[test]
    fn extract_auth_header_or_bearer_reads_bearer_fallback() {
        let req = "GET / HTTP/1.1\r\nAuthorization: Bearer fallback-token\r\n\r\n";
        let scheme = AuthScheme::HeaderOrBearer {
            header: "X-OmniLauncher-Token",
        };
        assert_eq!(extract_auth(req, scheme), Some("fallback-token"));
    }

    #[test]
    fn extract_auth_header_or_bearer_bearer_case_insensitive() {
        let req = "GET / HTTP/1.1\r\nAuthorization: bearer token-xyz\r\n\r\n";
        let scheme = AuthScheme::HeaderOrBearer {
            header: "X-OmniLauncher-Token",
        };
        assert_eq!(extract_auth(req, scheme), Some("token-xyz"));
        let req2 = "GET / HTTP/1.1\r\nAuthorization: BEARER token-xyz\r\n\r\n";
        assert_eq!(extract_auth(req2, scheme), Some("token-xyz"));
    }

    #[test]
    fn extract_auth_header_or_bearer_rejects_non_bearer_authorization() {
        let req = "GET / HTTP/1.1\r\nAuthorization: Basic dXNlcjpwYXNz\r\n\r\n";
        let scheme = AuthScheme::HeaderOrBearer {
            header: "X-OmniLauncher-Token",
        };
        assert_eq!(extract_auth(req, scheme), None);
    }

    #[test]
    fn extract_auth_header_or_bearer_custom_wins_when_both_present() {
        let scheme = AuthScheme::HeaderOrBearer {
            header: "X-OmniLauncher-Token",
        };
        let req = "GET / HTTP/1.1\r\nX-OmniLauncher-Token: custom-token\r\nAuthorization: Bearer bearer-token\r\n\r\n";
        assert_eq!(extract_auth(req, scheme), Some("custom-token"));
        // Order reversed → custom still wins.
        let req2 = "GET / HTTP/1.1\r\nAuthorization: Bearer bearer-token\r\nX-OmniLauncher-Token: custom-token\r\n\r\n";
        assert_eq!(extract_auth(req2, scheme), Some("custom-token"));
    }

    #[test]
    fn extract_auth_trims_surrounding_whitespace() {
        let req = "GET / HTTP/1.1\r\nX-OmniLauncher-Token:   spaced-token   \r\n\r\n";
        let scheme = AuthScheme::HeaderOrBearer {
            header: "X-OmniLauncher-Token",
        };
        assert_eq!(extract_auth(req, scheme), Some("spaced-token"));
    }

    #[test]
    fn extract_auth_stops_at_header_body_boundary() {
        // Auth-like header in body should NOT be picked up.
        let req = "POST /x HTTP/1.1\r\nHost: x\r\n\r\nAuthorization: Bearer leaked\r\n";
        let scheme = AuthScheme::HeaderOrBearer {
            header: "X-OmniLauncher-Token",
        };
        assert_eq!(extract_auth(req, scheme), None);
    }
}
