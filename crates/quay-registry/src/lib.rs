//! Client for an npm-compatible registry (defaults to registry.npmjs.org).
//!
//! Responsibilities: fetch package metadata (the "packument") and download
//! version tarballs. It does NOT decide *what* to install — that's the resolver.

use anyhow::Result;
use serde::Deserialize;

pub const DEFAULT_REGISTRY: &str = "https://registry.npmjs.org";

/// Thin async wrapper over an npm-compatible registry.
#[derive(Debug, Clone)]
pub struct RegistryClient {
    base_url: String,
    http: reqwest::Client,
}

/// The registry's metadata document for a package ("packument"), trimmed to
/// the fields the resolver needs.
#[derive(Debug, Clone, Deserialize)]
pub struct Packument {
    pub name: String,
    #[serde(default)]
    pub versions: std::collections::BTreeMap<String, VersionMeta>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionMeta {
    pub version: String,
    #[serde(default)]
    pub dependencies: std::collections::BTreeMap<String, String>,
    pub dist: Dist,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dist {
    pub tarball: String,
    /// Integrity hash (e.g. `sha512-...`), used to verify downloads.
    #[serde(default)]
    pub integrity: Option<String>,
}

impl RegistryClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::Client::new(),
        }
    }

    pub fn npm() -> Self {
        Self::new(DEFAULT_REGISTRY)
    }

    /// Fetch the full metadata document for a package.
    pub async fn packument(&self, name: &str) -> Result<Packument> {
        let url = format!("{}/{}", self.base_url, name);
        tracing::debug!(%url, "fetching packument");
        let pkg = self.http.get(url).send().await?.error_for_status()?.json().await?;
        Ok(pkg)
    }

    /// Download a tarball by URL, returning the raw `.tgz` bytes.
    ///
    /// TODO(quay): verify `integrity` before returning; stream to the store
    /// instead of buffering in memory for large packages.
    pub async fn download_tarball(&self, tarball_url: &str) -> Result<Vec<u8>> {
        let bytes = self
            .http
            .get(tarball_url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        Ok(bytes.to_vec())
    }
}
