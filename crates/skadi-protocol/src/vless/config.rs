use serde::Deserialize;
use std::fmt;

/// Идентификатор пользователя VLESS — 16-байтовый UUID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Uuid([u8; 16]);

impl Uuid {
    /// Разбор UUID из канонической строки "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx".
    pub fn parse(s: &str) -> Result<Self, UuidParseError> {
        let clean: String = s.chars().filter(|c| *c != '-').collect();
        if clean.len() != 32 {
            return Err(UuidParseError::BadLength(clean.len()));
        }

        let mut bytes = [0u8; 16];
        for i in 0..16 {
            let hi = hex_val(clean.as_bytes()[i * 2])?;
            let lo = hex_val(clean.as_bytes()[i * 2 + 1])?;
            bytes[i] = (hi << 4) | lo;
        }
        Ok(Uuid(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = &self.0;
        write!(
            f,
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-\
             {:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
            b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
        )
    }
}

fn hex_val(b: u8) -> Result<u8, UuidParseError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(UuidParseError::BadHex(b as char)),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UuidParseError {
    #[error("UUID must contain 32 hex digits, got {0}")]
    BadLength(usize),
    #[error("invalid hex character: {0}")]
    BadHex(char),
}

#[derive(Debug, Clone, Deserialize)]
pub struct VlessConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub users: Vec<VlessUser>,
}

impl Default for VlessConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            users: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct VlessUser {
    pub id: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub flow: Option<String>,
}

impl VlessConfig {
    /// Проверить UUID. Сравнение за постоянное время.
    pub fn authenticate(&self, candidate: &[u8; 16]) -> Option<&VlessUser> {
        use subtle::ConstantTimeEq;

        for user in &self.users {
            let Ok(uuid) = Uuid::parse(&user.id) else {
                continue;
            };
            if uuid.as_bytes().ct_eq(candidate).into() {
                return Some(user);
            }
        }
        None
    }
}
