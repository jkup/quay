//! The crate-wide error type. Keep variants specific so callers can match.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("manifest error: {0}")]
    Manifest(String),

    #[error("invalid package name: {0}")]
    InvalidPackageName(String),

    #[error("invalid version requirement `{0}`: {1}")]
    InvalidVersionReq(String, semver::Error),

    #[error("integrity check failed for `{package}`: expected {expected}, got {actual}")]
    IntegrityMismatch {
        package: String,
        expected: String,
        actual: String,
    },

    #[error("malformed integrity string `{0}`: {1}")]
    MalformedIntegrity(String, String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}
