//! Секреты и маскирование в логах.

use serde::{Deserialize, Deserializer};
use std::fmt;

pub const REDACTED: &str = "[REDACTED]";

/// Строка-секрет: не попадает в `Debug` / `Display`.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretString {
    inner: String,
}

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            inner: value.into(),
        }
    }

    pub fn expose(&self) -> &str {
        &self.inner
    }

    pub fn is_blank(&self) -> bool {
        self.inner.trim().is_empty()
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(")
            .and_then(|_| f.write_str(REDACTED))
            .and_then(|_| f.write_str(")"))
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(SecretString::new)
    }
}

/// Заменить `Bearer <token>` в тексте логов на `Bearer [REDACTED]`.
pub fn redact_bearer_tokens(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    const BEARER_PREFIX: &[u8] = b"bearer ";
    while i < bytes.len() {
        let end = i + BEARER_PREFIX.len();
        if end <= bytes.len() && bytes[i..end].eq_ignore_ascii_case(BEARER_PREFIX) {
            for &b in &bytes[i..end] {
                out.push(b as char);
            }
            i = end;
            while i < bytes.len() && !is_bearer_token_boundary(bytes[i]) {
                i += 1;
            }
            out.push_str(REDACTED);
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn is_bearer_token_boundary(byte: u8) -> bool {
    matches!(
        byte,
        b' ' | b'\t' | b'\n' | b'\r' | b'"' | b'\'' | b',' | b'}' | b']' | b'\\'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_string_masks_debug_and_display() {
        let secret = SecretString::new("super-secret-token");
        assert_eq!(format!("{secret:?}"), "SecretString([REDACTED])");
        assert_eq!(format!("{secret}"), REDACTED);
        assert_eq!(secret.expose(), "super-secret-token");
    }

    #[test]
    fn redact_bearer_tokens_in_log_line() {
        let line = r#"{"msg":"auth failed","authorization":"Bearer my-secret-token"}"#;
        let redacted = redact_bearer_tokens(line);
        assert!(!redacted.contains("my-secret-token"));
        assert!(redacted.contains("Bearer [REDACTED]"));
    }

    #[test]
    fn redact_bearer_case_insensitive() {
        let line = "authorization: bearer AbCdEf123";
        let redacted = redact_bearer_tokens(line);
        assert!(!redacted.contains("AbCdEf123"));
        assert!(redacted.contains("bearer [REDACTED]"));
    }

    #[test]
    fn redact_leaves_non_bearer_text() {
        let line = "connection opened peer=127.0.0.1:443";
        assert_eq!(redact_bearer_tokens(line), line);
    }
}
