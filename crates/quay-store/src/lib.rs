//! The content-addressable global store.
//!
//! Like pnpm, every package version is unpacked exactly once into a global
//! store keyed by content hash, then hard-linked into each project's
//! `node_modules`. This makes installs fast and disk-cheap across projects.

use std::path::{Path, PathBuf};

use anyhow::Result;

/// Handle to the on-disk global store (default: `~/.quay/store`).
#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Default store location, `~/.quay/store`.
    pub fn default_location() -> Result<Self> {
        let home = std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
        Ok(Self::new(Path::new(&home).join(".quay").join("store")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Unpack a tarball into the store under its content address and return the
    /// path it now lives at.
    ///
    /// TODO(quay) M2: compute content hash, extract `.tgz`, dedupe, then
    /// hard-link into the target `node_modules`.
    pub fn extract(&self, _tarball: &[u8], _name_for_log: &str) -> Result<PathBuf> {
        anyhow::bail!("store extraction not yet implemented (ROADMAP M2)")
    }
}
