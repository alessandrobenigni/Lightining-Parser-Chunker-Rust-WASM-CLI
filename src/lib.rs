pub mod chunking;
pub mod cli;
pub mod config;
pub mod format;
pub mod model;
pub mod orchestrator;
pub mod output;
pub mod vision;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("[E1001] unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("[E1002] not implemented: {0}")]
    NotImplemented(&'static str),

    #[error("[E2001] I/O error: {0}")]
    Io(String),

    #[error("[E3001] parse error: {0}")]
    Parse(String),

    #[error("[E4001] serialization error: {0}")]
    Serialization(String),

    #[error("[E5001] configuration error: {0}")]
    ConfigError(String),
}

impl Error {
    /// Return the structured error code for this error variant.
    pub fn code(&self) -> &'static str {
        match self {
            Error::UnsupportedFormat(_) => "E1001",
            Error::NotImplemented(_) => "E1002",
            Error::Io(_) => "E2001",
            Error::Parse(_) => "E3001",
            Error::Serialization(_) => "E4001",
            Error::ConfigError(_) => "E5001",
        }
    }
}
