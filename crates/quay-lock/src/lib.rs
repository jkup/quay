//! The `quay.lock` file: a deterministic, fully-pinned record of a resolution
//! so installs are reproducible across machines and CI.

use std::path::Path;

use anyhow::Result;
use quay_resolver::Resolution;
use serde::{Deserialize, Serialize};

pub const LOCKFILE_NAME: &str = "quay.lock";

/// Versioned on-disk lockfile format. Bump `lockfile_version` on breaking
/// format changes so older Quay versions can refuse gracefully.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    pub lockfile_version: u32,
    pub packages: std::collections::BTreeMap<String, LockedPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedPackage {
    pub version: String,
    pub resolved: String,
    pub integrity: Option<String>,
}

impl Lockfile {
    /// Build a lockfile from a solved resolution.
    ///
    /// TODO(quay) M1-3: emit the real `resolved`/`integrity` values now carried
    /// on [`ResolvedPackage`]; for now they stay empty so the format is stable.
    pub fn from_resolution(resolution: &Resolution) -> Self {
        let packages = resolution
            .packages
            .iter()
            .map(|(key, pkg)| {
                (
                    key.clone(),
                    LockedPackage {
                        version: pkg.id.version.to_string(),
                        resolved: String::new(),
                        integrity: None,
                    },
                )
            })
            .collect();
        Self {
            lockfile_version: 1,
            packages,
        }
    }

    pub fn save(&self, dir: impl AsRef<Path>) -> Result<()> {
        let path = dir.as_ref().join(LOCKFILE_NAME);
        let json = serde_json::to_vec_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load(dir: impl AsRef<Path>) -> Result<Self> {
        let path = dir.as_ref().join(LOCKFILE_NAME);
        let bytes = std::fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}
