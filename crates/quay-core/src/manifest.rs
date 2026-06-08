//! `package.json` parsing. Quay is npm-compatible by design: we read the same
//! manifest the rest of the ecosystem uses, so existing projects work unchanged.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// A subset of `package.json`. Unknown fields are preserved on round-trip via
/// `extra` so we never clobber config we don't yet understand.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub name: Option<String>,
    pub version: Option<String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, String>,

    #[serde(
        default,
        rename = "devDependencies",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub dev_dependencies: BTreeMap<String, String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scripts: BTreeMap<String, String>,

    /// Everything else in package.json we don't model yet, kept verbatim.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Manifest {
    /// Load and parse a `package.json` from disk.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// All runtime + dev dependencies as `(name, version_req)` pairs.
    pub fn all_dependencies(&self) -> impl Iterator<Item = (&String, &String)> {
        self.dependencies.iter().chain(self.dev_dependencies.iter())
    }
}
