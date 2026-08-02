use thiserror::Error;

/// Errors produced by the MCP client library.
#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error(
        "authentication required: provide --token/MCP_TOKEN or use --login"
    )]
    AuthRequired,

    #[error("OAuth error: {0}")]
    Oauth(String),

    #[error("MCP transport/service error: {0}")]
    Service(String),

    #[error("invalid tool arguments JSON: {0}")]
    InvalidArguments(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn service(err: impl std::fmt::Display) -> Self {
        Self::Service(err.to_string())
    }

    pub fn oauth(err: impl std::fmt::Display) -> Self {
        Self::Oauth(err.to_string())
    }

    pub fn other(err: impl std::fmt::Display) -> Self {
        Self::Other(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
