//! Client for an npm-compatible registry (defaults to registry.npmjs.org).
//!
//! Responsibilities: fetch package metadata (the "packument") and download
//! version tarballs. It does NOT decide *what* to install — that's the resolver.

use std::future::Future;

use anyhow::Result;
use serde::Deserialize;

pub const DEFAULT_REGISTRY: &str = "https://registry.npmjs.org";

/// A source of package metadata: anything that can hand back a [`Packument`]
/// for a package name.
///
/// This is the seam the resolver depends on. [`RegistryClient`] is the real,
/// network-backed implementation; tests provide an in-memory mock so the
/// resolver can be exercised without touching the network.
///
/// The return type is spelled out as `impl Future + Send` (rather than
/// `async fn`) to avoid the `async_fn_in_trait` lint and to keep the future
/// `Send`, which callers on multi-threaded executors need.
pub trait PackageSource {
    /// Fetch the full metadata document for a package.
    fn packument(&self, name: &str) -> impl Future<Output = Result<Packument>> + Send;
}

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

impl PackageSource for RegistryClient {
    /// Fetch the full metadata document for a package over HTTP.
    async fn packument(&self, name: &str) -> Result<Packument> {
        let url = format!("{}/{}", self.base_url, name);
        tracing::debug!(%url, "fetching packument");
        let pkg = self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(pkg)
    }
}
