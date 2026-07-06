//! Mask sensitive values in data we log.
//!
//! See `docs/superpowers/specs/2026-06-10-credential-masking-in-logs-design.md`
//! for the design rationale, detection rules, and audited call sites.

use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

const REDACTED: &str = "***";

/// Field-name pattern — case-insensitive, full match against an object key.
/// When a key matches this, the value is replaced with `***` outright.
fn key_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^(private[_-]?key|api[_-]?key|secret|password|passwd|token|authorization|credentials?|client[_-]?secret|access[_-]?key|refresh[_-]?token|session[_-]?token|bearer)$",
        )
        .expect("key denylist regex compiles")
    })
}

/// Long-flag pattern for argv pairs: matches `--password`, `-Password`,
/// `--api-key`, etc. (case-insensitive). When an argv element matches
/// this, the *next* element is replaced with `***`.
fn flag_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^-{1,2}(password|passwd|token|api[-_]?key|secret|authorization|bearer|access[-_]?key|client[-_]?secret|credential)s?$",
        )
        .expect("flag denylist regex compiles")
    })
}

/// Value-content patterns that look like secrets regardless of where
/// they appear. Applied to every string we serialize.
fn value_patterns() -> &'static [Regex] {
    static RES: OnceLock<Vec<Regex>> = OnceLock::new();
    RES.get_or_init(|| {
        vec![
            // PEM-encoded private key blocks (RSA, EC, plain, etc.).
            Regex::new(
                r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
            )
            .expect("PEM regex compiles"),
            // JWTs: three base64url segments joined by '.'. Min 10 chars per
            // segment keeps it from matching ordinary dotted identifiers.
            Regex::new(r"eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}")
                .expect("JWT regex compiles"),
            // GitHub tokens: ghp_, gho_, ghu_, ghs_, ghr_.
            Regex::new(r"gh[pousr]_[A-Za-z0-9]{36,}").expect("GitHub PAT regex compiles"),
            // AWS access key ids.
            Regex::new(r"AKIA[0-9A-Z]{16}").expect("AWS key regex compiles"),
            // Bearer header values with a long opaque token.
            Regex::new(r"(?i)bearer\s+[A-Za-z0-9._\-+/=]{20,}").expect("bearer regex compiles"),
        ]
    })
}

/// Walk `value` and mask sensitive fields, then serialize as compact JSON.
pub fn mask_json(value: &Value) -> String {
    let masked = mask_value(value);
    serde_json::to_string(&masked).unwrap_or_else(|_| "<unserializable>".to_string())
}

/// Run the value-pattern sweep over a free-form string.
pub fn mask_str(input: &str) -> String {
    let mut out = input.to_string();
    for re in value_patterns() {
        out = re.replace_all(&out, REDACTED).into_owned();
    }
    out
}

/// Mask each element of an argv array and join with spaces.
///
/// Two sweeps:
/// 1. **Value sweep**: each element goes through `mask_str`.
/// 2. **Flag-pair sweep**: when an element matches the long-flag denylist
///    (e.g. `--password`, `--api-key`), the following element is replaced
///    with `***`.
pub fn mask_argv<S: AsRef<str>>(args: &[S]) -> String {
    let mut out: Vec<String> = args.iter().map(|a| mask_str(a.as_ref())).collect();
    let flag = flag_pattern();
    let mut i = 0;
    while i + 1 < out.len() {
        if flag.is_match(args[i].as_ref()) {
            out[i + 1] = REDACTED.to_string();
            i += 2;
        } else {
            i += 1;
        }
    }
    out.join(" ")
}

fn mask_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let key_re = key_pattern();
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                if key_re.is_match(k) {
                    out.insert(k.clone(), Value::String(REDACTED.to_string()));
                } else {
                    out.insert(k.clone(), mask_value(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(mask_value).collect()),
        Value::String(s) => Value::String(mask_str(s)),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn mask_json_redacts_private_key_field() {
        let input = json!({
            "type": "service_account",
            "project_id": "blz-d-cf-probe-1c",
            "private_key": "-----BEGIN PRIVATE KEY-----\nMIIB\n-----END PRIVATE KEY-----\n",
            "client_email": "foo@bar.iam.gserviceaccount.com"
        });
        let masked = mask_json(&input);
        assert!(!masked.contains("MIIB"), "raw key body must not appear");
        assert!(
            masked.contains("\"private_key\":\"***\""),
            "expected redacted private_key, got: {masked}"
        );
        assert!(
            masked.contains("\"project_id\":\"blz-d-cf-probe-1c\""),
            "non-sensitive field preserved"
        );
        assert!(
            masked.contains("\"client_email\":\"foo@bar.iam.gserviceaccount.com\""),
            "non-sensitive field preserved"
        );
    }

    #[test]
    fn mask_json_redacts_nested_credentials() {
        let input = json!({
            "creds": { "api_key": "abcdef123456" },
            "name": "thing"
        });
        let masked = mask_json(&input);
        assert!(!masked.contains("abcdef123456"));
        assert!(masked.contains("\"api_key\":\"***\""));
        assert!(masked.contains("\"name\":\"thing\""));
    }

    #[test]
    fn mask_json_redacts_pem_inside_benign_field() {
        let key = "-----BEGIN PRIVATE KEY-----\nMIIEvgIBADAN\n-----END PRIVATE KEY-----\n";
        let input = json!({
            "path": "/tmp/gcp_sa.json",
            "content": format!("{{\n  \"private_key\": \"{}\"\n}}", key.replace('\n', "\\n"))
        });
        let masked = mask_json(&input);
        assert!(
            !masked.contains("MIIEvgIBADAN"),
            "PEM body leaked: {masked}"
        );
        assert!(
            masked.contains("\"path\":\"/tmp/gcp_sa.json\""),
            "path preserved"
        );
    }

    #[test]
    fn mask_json_redacts_jwt() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let input = json!({ "message": format!("token={}", jwt) });
        let masked = mask_json(&input);
        assert!(!masked.contains(jwt), "JWT leaked: {masked}");
    }

    #[test]
    fn mask_json_redacts_bearer_value() {
        let input = json!({
            "headers": { "Authorization": "Bearer eyJabcdefghij1234567890klmno" }
        });
        let masked = mask_json(&input);
        assert!(
            masked.contains("\"Authorization\":\"***\""),
            "got: {masked}"
        );
        assert!(!masked.contains("eyJabcdefghij"));
    }

    #[test]
    fn mask_json_preserves_innocent_payload() {
        let input = json!({
            "path": "/tmp/foo.txt",
            "append": false,
            "count": 7,
            "items": ["alpha", "beta"]
        });
        let masked = mask_json(&input);
        assert!(masked.contains("\"path\":\"/tmp/foo.txt\""));
        assert!(masked.contains("\"append\":false"));
        assert!(masked.contains("\"count\":7"));
        assert!(masked.contains("\"items\":[\"alpha\",\"beta\"]"));
    }

    #[test]
    fn mask_str_redacts_pem_block() {
        let s = "before -----BEGIN PRIVATE KEY-----\nSECRET\n-----END PRIVATE KEY----- after";
        let masked = mask_str(s);
        assert!(!masked.contains("SECRET"));
        assert!(masked.starts_with("before "));
        assert!(masked.ends_with(" after"));
    }

    #[test]
    fn mask_argv_flag_pair_password() {
        let argv = ["--user", "alice", "--password", "hunter2", "--verbose"];
        let masked = mask_argv(&argv);
        assert!(
            masked.contains("--user alice"),
            "non-secret pair preserved: {masked}"
        );
        assert!(
            masked.contains("--password ***"),
            "secret-following flag redacted: {masked}"
        );
        assert!(!masked.contains("hunter2"));
        assert!(masked.contains("--verbose"));
    }

    #[test]
    fn mask_argv_redacts_bearer_inline() {
        let argv = [
            "curl",
            "-H",
            "Authorization: Bearer eyJabcdefghij1234567890klmno",
        ];
        let masked = mask_argv(&argv);
        assert!(!masked.contains("eyJabcdefghij1234567890klmno"));
        assert!(masked.contains("curl"));
    }

    #[test]
    fn mask_argv_case_insensitive_flag() {
        let argv = ["--Token", "abcd1234efgh5678"];
        let masked = mask_argv(&argv);
        assert!(masked.contains("--Token ***"), "got: {masked}");
        assert!(!masked.contains("abcd1234efgh5678"));
    }

    #[test]
    fn mask_argv_short_flag_pair() {
        // Boundary test: short `-p` is intentionally NOT in the flag denylist
        // because it conflicts with other tools (e.g. `mkdir -p`). Document the
        // boundary so a future refactor doesn't silently change behavior.
        let argv = ["mysql", "-p", "hunter2"];
        let masked = mask_argv(&argv);
        assert!(
            masked.contains("hunter2"),
            "short -p not in denylist: {masked}"
        );
    }
}
