//! Secret scrubbing. Used on every string that could reach a report, a log or an LLM prompt, and
//! (in `har`) on all captured traffic BEFORE inference. `pii_filter` in the app covers e-mail/phone/
//! card/Aadhaar/PAN/IPv4 but not tokens, JWTs, cookies or API keys - this module does.

use regex::Regex;
use std::sync::OnceLock;

pub const REDACTED: &str = "[REDACTED]";

fn rx(cell: &'static OnceLock<Vec<(Regex, &'static str)>>) -> &'static Vec<(Regex, &'static str)> {
    cell.get_or_init(|| {
        let pats: &[(&str, &str)] = &[
            // PEM private keys (whole block, or just the header if truncated)
            (r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?(-----END [A-Z ]*PRIVATE KEY-----|$)", "[REDACTED:PRIVATE_KEY]"),
            // JWT: three base64url segments, first starts with eyJ
            (r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{4,}", "[REDACTED:JWT]"),
            // provider-shaped keys
            (r"\bAKIA[0-9A-Z]{16}\b", "[REDACTED:AWS_KEY]"),
            (r"\bgh[pousr]_[A-Za-z0-9]{20,}\b", "[REDACTED:GITHUB_TOKEN]"),
            (r"\bxox[abprs]-[A-Za-z0-9-]{10,}", "[REDACTED:SLACK_TOKEN]"),
            (r"\bAIza[0-9A-Za-z_-]{35}\b", "[REDACTED:GOOGLE_KEY]"),
            (r"\bsk-[A-Za-z0-9_-]{20,}", "[REDACTED:API_KEY]"),
            (r"\bgsk_[A-Za-z0-9]{20,}", "[REDACTED:API_KEY]"),
            // Authorization / Cookie style headers and bearer tokens
            (r"(?i)\b(authorization|proxy-authorization|x-api-key|x-auth-token|x-csrf-token)\s*[:=]\s*[^\r\n]+", "$1: [REDACTED]"),
            (r"(?i)\b(set-cookie|cookie)\s*[:=]\s*[^\r\n]+", "$1: [REDACTED]"),
            (r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{8,}", "Bearer [REDACTED]"),
            (r"(?i)\bbasic\s+[A-Za-z0-9+/=]{8,}", "Basic [REDACTED]"),
            // key=value / key: value / "key":"value" with a secret-ish key name
            (
                r#"(?i)(["']?[a-z0-9_.-]*(?:api[_-]?key|apikey|secret|token|passw(?:or)?d|pwd|passphrase|session[_-]?id|sessionid|signature|credential|private[_-]?key|access[_-]?key)[a-z0-9_.-]*["']?\s*[:=]\s*)(?:"[^"\r\n]{1,512}"|'[^'\r\n]{1,512}'|[^\s&;,"'<>]{1,512})"#,
                "$1\"[REDACTED]\"",
            ),
        ];
        pats.iter().map(|(p, r)| (Regex::new(p).expect("redact regex"), *r)).collect()
    })
}

/// Remove secrets from free text. Idempotent.
pub fn redact_text(text: &str) -> String {
    static CELL: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    let mut out = text.to_string();
    for (re, rep) in rx(&CELL) {
        out = re.replace_all(&out, *rep).into_owned();
    }
    out
}

/// Stricter pass for text that leaves the device (LLM prompts): secrets + e-mail addresses +
/// the user name inside `C:\Users\<name>` / `/home/<name>` paths.
pub fn redact_for_llm(text: &str) -> String {
    static EMAIL: OnceLock<Regex> = OnceLock::new();
    static HOME: OnceLock<Regex> = OnceLock::new();
    let email = EMAIL.get_or_init(|| Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").unwrap());
    let home = HOME.get_or_init(|| Regex::new(r"(?i)((?:[a-z]:)?[\\/]+(?:users|home)[\\/]+)[^\\/\s]+").unwrap());
    let t = redact_text(text);
    let t = email.replace_all(&t, "[REDACTED:EMAIL]").into_owned();
    home.replace_all(&t, "${1}<user>").into_owned()
}

/// Is this header / query / JSON-field NAME one whose VALUE must never be kept?
pub fn is_secret_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    const NEEDLES: &[&str] = &[
        "authorization", "cookie", "token", "secret", "password", "passwd", "pwd", "passphrase", "apikey", "api-key",
        "api_key", "x-api", "session", "signature", "credential", "csrf", "xsrf", "otp", "jwt", "bearer", "privatekey",
        "private_key", "private-key", "access_key", "access-key", "accesskey", "auth",
    ];
    NEEDLES.iter().any(|k| n.contains(k))
}

/// Does this value LOOK like a credential even under an innocent name (JWT, long hex/base64 blob)?
pub fn looks_like_secret_value(v: &str) -> bool {
    static JWT: OnceLock<Regex> = OnceLock::new();
    let jwt = JWT.get_or_init(|| Regex::new(r"^eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{4,}$").unwrap());
    let t = v.trim();
    if jwt.is_match(t) {
        return true;
    }
    if t.len() >= 32 && t.len() <= 4096 && t.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b'-' | b'_')) {
        // long unbroken token-like blob; UUIDs have dashes at fixed places and are NOT treated as secrets
        let digits = t.bytes().filter(u8::is_ascii_digit).count();
        let uppers = t.bytes().filter(u8::is_ascii_uppercase).count();
        let lowers = t.bytes().filter(u8::is_ascii_lowercase).count();
        let is_uuid = t.len() == 36 && t.matches('-').count() == 4;
        return !is_uuid && ((digits > 0 && (uppers + lowers) > 0) || (uppers > 0 && lowers > 0));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const JWT: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dBjftJeZ4CVPmB92K27uhbUJU1p1r_wW1gFWFOEjXk";

    #[test]
    fn redacts_the_common_secret_shapes() {
        let cases = [
            (format!("token is {JWT} ok"), JWT),
            ("Authorization: Bearer abc123def456ghi789".to_string(), "abc123def456"),
            ("Cookie: sid=9f8e7d6c5b4a; theme=dark".to_string(), "9f8e7d6c5b4a"),
            ("Set-Cookie: session=SUPERSECRETVALUE; Path=/".to_string(), "SUPERSECRETVALUE"),
            ("key AKIAIOSFODNN7EXAMPLE leaked".to_string(), "AKIAIOSFODNN7EXAMPLE"),
            ("ghp_abcdefghijklmnopqrstuvwxyz0123456789".to_string(), "ghp_abcdefghijklmnop"),
            ("AIzaSyA-1234567890abcdefghijklmnopqrstu".to_string(), "AIzaSyA-1234567890"),
            ("sk-abcdefghijklmnopqrstuvwxyz123456".to_string(), "sk-abcdefghijklmnop"),
            ("password=hunter2hunter2&user=bob".to_string(), "hunter2hunter2"),
            (r#"{"api_key": "AbCdEf1234567890", "name": "x"}"#.to_string(), "AbCdEf1234567890"),
            ("-----BEGIN RSA PRIVATE KEY-----\nMIIEow\n-----END RSA PRIVATE KEY-----".to_string(), "MIIEow"),
            ("x-api-key: 0123456789abcdef0123".to_string(), "0123456789abcdef0123"),
        ];
        for (input, secret) in cases {
            let out = redact_text(&input);
            assert!(!out.contains(secret), "leaked {secret:?} in {out:?}");
            assert!(out.contains("REDACTED"), "no marker in {out:?}");
        }
    }

    #[test]
    fn leaves_ordinary_text_alone_and_is_idempotent() {
        let s = "GET https://example.test/v1/users?page=2 returned 200 in 31 ms";
        assert_eq!(redact_text(s), s);
        let once = redact_text("password=abcdefgh12345");
        assert_eq!(redact_text(&once), once);
    }

    #[test]
    fn llm_pass_also_scrubs_email_and_home_dirs() {
        let out = redact_for_llm(r"C:\Users\lakshya\build\app.pdb mail me at a.b@example.com");
        assert!(!out.contains("lakshya") && !out.contains("a.b@example.com"), "{out}");
        assert!(out.contains(r"C:\Users\<user>"));
        assert!(redact_for_llm("/home/alice/x").contains("/home/<user>"));
    }

    #[test]
    fn secret_names_and_values() {
        for n in ["Authorization", "X-Api-Key", "set-cookie", "access_token", "client_secret", "Password", "sessionId", "csrfToken"] {
            assert!(is_secret_name(n), "{n}");
        }
        for n in ["content-type", "page", "name", "limit", "user-agent"] {
            assert!(!is_secret_name(n), "{n}");
        }
        assert!(looks_like_secret_value(JWT));
        assert!(looks_like_secret_value("aB3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA5"));
        assert!(!looks_like_secret_value("550e8400-e29b-41d4-a716-446655440000")); // UUID
        assert!(!looks_like_secret_value("hello world"));
        assert!(!looks_like_secret_value("12345"));
    }
}
