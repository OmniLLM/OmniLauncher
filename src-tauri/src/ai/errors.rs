/// Structured error type for all AI client operations.
#[derive(Debug, thiserror::Error)]
pub enum AiError {
    /// HTTP client / transport error (connection refused, DNS failure, etc.)
    #[error("HTTP transport error: {0}")]
    Transport(String),

    /// The server responded with a non-2xx status.
    #[error("API error {status}: {body}")]
    Api { status: u16, body: String },

    /// JSON serialization / deserialization failure.
    #[error("JSON error: {0}")]
    Json(String),

    /// Request timed out.
    #[error("request timed out")]
    Timeout,

    /// The AI model produced malformed output (bad tool call, etc.)
    #[error("model error: {0}")]
    Model(String),

    /// Context / token budget exceeded.
    #[error("context limit exceeded: {0}")]
    ContextLimit(String),
}

impl AiError {
    /// Convert from a raw reqwest error string (used during migration from String errors).
    pub fn from_reqwest_str(s: &str) -> Self {
        if s.contains("timed out") || s.contains("timeout") {
            AiError::Timeout
        } else {
            AiError::Transport(s.to_string())
        }
    }

    /// True for errors that indicate a non-2xx HTTP response with a specific status.
    pub fn status(&self) -> Option<u16> {
        match self {
            AiError::Api { status, .. } => Some(*status),
            _ => None,
        }
    }
}

/// Classify an `AiError` into an `ErrorClass` for retry/recovery logic.
pub fn classify_ai_error(err: &AiError) -> ErrorClass {
    match err {
        AiError::Timeout => ErrorClass::Transient,
        AiError::Transport(_) => ErrorClass::Transient,
        AiError::Api { status, .. } => match status {
            429 | 502 | 503 => ErrorClass::Transient,
            400 | 401 | 403 | 404 | 422 => ErrorClass::Permanent,
            _ => ErrorClass::Permanent,
        },
        AiError::Json(_) => ErrorClass::Permanent,
        AiError::Model(_) => ErrorClass::ModelError,
        AiError::ContextLimit(_) => ErrorClass::ResourceError,
    }
}

/// Classification of AI/network errors to guide retry and recovery strategies.
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorClass {
    /// Temporary failure — may succeed if retried (timeout, rate limit, 429, 502, 503).
    Transient,
    /// Unrecoverable failure — do not retry (auth error, bad request, etc.).
    Permanent,
    /// The model produced invalid output (bad tool call, malformed function, etc.).
    ModelError,
    /// Context/token budget exceeded — compress and retry.
    ResourceError,
}

/// Classify an error message string into an `ErrorClass`.
pub fn classify_error(msg: &str) -> ErrorClass {
    let lower = msg.to_lowercase();
    if lower.contains("timeout")
        || lower.contains("rate limit")
        || lower.contains("429")
        || lower.contains("503")
        || lower.contains("502")
    {
        ErrorClass::Transient
    } else if (lower.contains("token") || lower.contains("context"))
        && (lower.contains("limit")
            || lower.contains("budget")
            || lower.contains("length")
            || lower.contains("exceeded"))
    {
        ErrorClass::ResourceError
    } else if lower.contains("invalid tool")
        || lower.contains("malformed")
        || lower.contains("function not found")
    {
        ErrorClass::ModelError
    } else {
        ErrorClass::Permanent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_transient() {
        assert_eq!(
            classify_error("timeout waiting for server"),
            ErrorClass::Transient
        );
        assert_eq!(classify_error("rate limit exceeded"), ErrorClass::Transient);
        assert_eq!(
            classify_error("API error 429: too many requests"),
            ErrorClass::Transient
        );
        assert_eq!(
            classify_error("API error 503: service unavailable"),
            ErrorClass::Transient
        );
        assert_eq!(
            classify_error("API error 502: bad gateway"),
            ErrorClass::Transient
        );
    }

    #[test]
    fn test_classify_resource() {
        assert_eq!(
            classify_error("token limit exceeded"),
            ErrorClass::ResourceError
        );
        assert_eq!(
            classify_error("token budget exhausted"),
            ErrorClass::ResourceError
        );
        assert_eq!(
            classify_error("context length exceeded"),
            ErrorClass::ResourceError
        );
    }

    #[test]
    fn test_classify_model() {
        assert_eq!(classify_error("invalid tool call"), ErrorClass::ModelError);
        assert_eq!(
            classify_error("malformed json in response"),
            ErrorClass::ModelError
        );
        assert_eq!(
            classify_error("function not found: foo"),
            ErrorClass::ModelError
        );
    }

    #[test]
    fn test_classify_permanent() {
        assert_eq!(
            classify_error("API error 401: unauthorized"),
            ErrorClass::Permanent
        );
        assert_eq!(
            classify_error("API error 400: bad request"),
            ErrorClass::Permanent
        );
        assert_eq!(classify_error("some unknown error"), ErrorClass::Permanent);
    }
}

#[cfg(test)]
mod ai_error_tests {
    use super::*;

    #[test]
    fn test_ai_error_display_api() {
        let e = AiError::Api {
            status: 429,
            body: "rate limited".into(),
        };
        assert!(e.to_string().contains("429"));
        assert!(e.to_string().contains("rate limited"));
    }

    #[test]
    fn test_ai_error_display_timeout() {
        let e = AiError::Timeout;
        assert!(e.to_string().contains("timed out"));
    }

    #[test]
    fn test_ai_error_classify_transient() {
        let e = AiError::Api {
            status: 429,
            body: "too many".into(),
        };
        assert_eq!(classify_ai_error(&e), ErrorClass::Transient);
    }

    #[test]
    fn test_ai_error_classify_permanent() {
        let e = AiError::Api {
            status: 401,
            body: "unauthorized".into(),
        };
        assert_eq!(classify_ai_error(&e), ErrorClass::Permanent);
    }
}
