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
    /// Each entry carries the pinned version plus the `resolved` tarball URL and
    /// `integrity` hash the resolver pulled from the registry, so an install is
    /// fully reproducible without re-querying version metadata. The
    /// [`BTreeMap`](std::collections::BTreeMap) keying keeps the output
    /// deterministic across runs.
    pub fn from_resolution(resolution: &Resolution) -> Self {
        let packages = resolution
            .packages
            .iter()
            .map(|(key, pkg)| {
                (
                    key.clone(),
                    LockedPackage {
                        version: pkg.id.version.to_string(),
                        resolved: pkg.tarball.clone(),
                        integrity: pkg.integrity.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use quay_core::{PackageId, PackageName};
    use quay_resolver::{Resolution, ResolvedPackage};
    use semver::Version;

    /// Build a [`Resolution`] with one entry per `(name, version, tarball,
    /// integrity)` tuple, keyed by name like the solver produces.
    fn resolution(entries: &[(&str, &str, &str, Option<&str>)]) -> Resolution {
        let packages = entries
            .iter()
            .map(|(name, version, tarball, integrity)| {
                let pkg = ResolvedPackage {
                    id: PackageId {
                        name: PackageName::parse(*name).unwrap(),
                        version: Version::parse(version).unwrap(),
                    },
                    tarball: (*tarball).to_string(),
                    integrity: integrity.map(str::to_string),
                };
                (name.to_string(), pkg)
            })
            .collect();
        Resolution { packages }
    }

    #[test]
    fn from_resolution_threads_tarball_and_integrity() {
        let res = resolution(&[(
            "left-pad",
            "1.3.0",
            "https://registry.npmjs.org/left-pad/-/left-pad-1.3.0.tgz",
            Some("sha512-deadbeef"),
        )]);

        let lock = Lockfile::from_resolution(&res);
        let entry = &lock.packages["left-pad"];

        assert_eq!(entry.version, "1.3.0");
        assert_eq!(
            entry.resolved,
            "https://registry.npmjs.org/left-pad/-/left-pad-1.3.0.tgz"
        );
        assert_eq!(entry.integrity.as_deref(), Some("sha512-deadbeef"));
    }

    #[test]
    fn missing_integrity_stays_none() {
        let res = resolution(&[(
            "no-hash",
            "0.1.0",
            "https://registry.example/no-hash-0.1.0.tgz",
            None,
        )]);

        let lock = Lockfile::from_resolution(&res);
        let entry = &lock.packages["no-hash"];

        assert_eq!(entry.resolved, "https://registry.example/no-hash-0.1.0.tgz");
        assert_eq!(entry.integrity, None);
    }

    #[test]
    fn save_load_round_trip_preserves_resolved_and_integrity() {
        let res = resolution(&[
            (
                "a",
                "1.0.0",
                "https://registry.npmjs.org/a/-/a-1.0.0.tgz",
                Some("sha512-aaa"),
            ),
            (
                "b",
                "2.5.1",
                "https://registry.npmjs.org/b/-/b-2.5.1.tgz",
                None,
            ),
        ]);

        let lock = Lockfile::from_resolution(&res);
        let dir = tempfile::tempdir().unwrap();
        lock.save(dir.path()).unwrap();
        let loaded = Lockfile::load(dir.path()).unwrap();

        assert_eq!(loaded.lockfile_version, lock.lockfile_version);
        assert_eq!(loaded.packages.len(), 2);

        let a = &loaded.packages["a"];
        assert_eq!(a.version, "1.0.0");
        assert_eq!(a.resolved, "https://registry.npmjs.org/a/-/a-1.0.0.tgz");
        assert_eq!(a.integrity.as_deref(), Some("sha512-aaa"));

        let b = &loaded.packages["b"];
        assert_eq!(b.version, "2.5.1");
        assert_eq!(b.resolved, "https://registry.npmjs.org/b/-/b-2.5.1.tgz");
        assert_eq!(b.integrity, None);
    }

    #[test]
    fn save_output_is_deterministic() {
        // BTreeMap keying means the same resolution always serializes byte-for-byte
        // the same, regardless of insertion order.
        let forward = resolution(&[
            ("a", "1.0.0", "https://r/a.tgz", Some("sha512-a")),
            ("b", "1.0.0", "https://r/b.tgz", Some("sha512-b")),
        ]);
        let reverse = resolution(&[
            ("b", "1.0.0", "https://r/b.tgz", Some("sha512-b")),
            ("a", "1.0.0", "https://r/a.tgz", Some("sha512-a")),
        ]);

        let one = serde_json::to_vec_pretty(&Lockfile::from_resolution(&forward)).unwrap();
        let two = serde_json::to_vec_pretty(&Lockfile::from_resolution(&reverse)).unwrap();
        assert_eq!(one, two);
    }
}
