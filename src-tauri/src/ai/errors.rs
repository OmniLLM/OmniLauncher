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
