//! Infrastructure-level errors: the "the operation could not even be attempted"
//! tier (IO, malformed config/DB, transport failures).
//!
//! Expected, "business" failures for a specific package operation (already
//! installed, source disabled, no matching release, …) are **not** modelled
//! here — see [`crate::results::ManagerError`].

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}
