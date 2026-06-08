//! Shared types for Quay: package identities, manifests, and the error type.
//!
//! Everything here is intentionally dependency-light. Other crates build on
//! these primitives; nothing in here reaches out to the network or filesystem.

pub mod error;
pub mod manifest;
pub mod package;

pub use error::{Error, Result};
pub use manifest::Manifest;
pub use package::{PackageId, PackageName};
