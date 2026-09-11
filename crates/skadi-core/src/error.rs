use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("authentication failed")]
    AuthFailed,

    #[error("unknown user: {0}")]
    UnknownUser(String),

    #[error("connection timeout")]
    Timeout,

    #[error("connection limit exceeded")]
    TooManyConnections,

    #[error("configuration error: {0}")]
    Config(String),
}
