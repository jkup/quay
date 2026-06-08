//! Dependency resolution: turn a set of version *requirements* into an exact,
//! conflict-free set of package versions (the resolution graph).
//!
//! The intended algorithm is PubGrub-style version solving (the same family
//! pnpm/cargo/uv use) for fast, high-quality error messages on conflicts.
//! This module currently exposes the shape; the solver itself is the first
//! big task for the agent — see ROADMAP.md, milestone M1.

use std::collections::BTreeMap;

use quay_core::PackageId;
use quay_registry::PackageSource;

/// A single resolved package: a pinned [`PackageId`] together with everything
/// the installer needs to fetch and verify it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPackage {
    /// The package name pinned to an exact version.
    pub id: PackageId,
    /// URL of the `.tgz` tarball to download from the registry.
    pub tarball: String,
    /// Integrity hash (e.g. `sha512-...`) for verifying the download, if the
    /// registry advertised one.
    pub integrity: Option<String>,
}

/// A resolved dependency graph: every package pinned to one exact version,
/// keyed by package name.
#[derive(Debug, Default, Clone)]
pub struct Resolution {
    pub packages: BTreeMap<String, ResolvedPackage>,
}

/// Resolve a manifest's direct dependencies (and, eventually, transitive ones)
/// into a fully pinned [`Resolution`].
///
/// `direct` is `(name, version_req)` straight from package.json. The package
/// metadata comes from any [`PackageSource`], so the resolver can be unit
/// tested against an in-memory mock instead of the live network.
pub async fn resolve<S: PackageSource>(
    _source: &S,
    direct: &[(String, String)],
) -> anyhow::Result<Resolution> {
    // TODO(quay) M1-2: implement PubGrub solving over registry packuments.
    // The `PackageSource` seam and the enriched `ResolvedPackage` model are in
    // place; the solver that fills `Resolution::packages` lands next.
    tracing::warn!("resolver is a stub: returning empty resolution");
    let _ = direct;
    Ok(Resolution::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use quay_registry::{Dist, Packument, VersionMeta};

    /// An in-memory [`PackageSource`] for tests: hands back canned packuments
    /// with no network access. Unknown names error like a 404 would.
    #[derive(Default)]
    struct MockSource {
        packuments: BTreeMap<String, Packument>,
    }

    impl MockSource {
        fn with(mut self, packument: Packument) -> Self {
            self.packuments.insert(packument.name.clone(), packument);
            self
        }
    }

    impl PackageSource for MockSource {
        async fn packument(&self, name: &str) -> anyhow::Result<Packument> {
            self.packuments
                .get(name)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("no such package: {name}"))
        }
    }

    fn packument(name: &str, version: &str, tarball: &str) -> Packument {
        let mut versions = BTreeMap::new();
        versions.insert(
            version.to_string(),
            VersionMeta {
                version: version.to_string(),
                dependencies: BTreeMap::new(),
                dist: Dist {
                    tarball: tarball.to_string(),
                    integrity: Some("sha512-deadbeef".to_string()),
                },
            },
        );
        Packument {
            name: name.to_string(),
            versions,
        }
    }

    #[tokio::test]
    async fn mock_source_serves_canned_packuments() {
        let source = MockSource::default().with(packument(
            "left-pad",
            "1.3.0",
            "https://registry.example/left-pad-1.3.0.tgz",
        ));

        let pkg = source.packument("left-pad").await.unwrap();
        assert_eq!(pkg.name, "left-pad");
        let meta = pkg.versions.get("1.3.0").expect("version present");
        assert_eq!(meta.dist.tarball, "https://registry.example/left-pad-1.3.0.tgz");

        // Unknown packages error rather than hitting the network.
        assert!(source.packument("does-not-exist").await.is_err());
    }

    #[tokio::test]
    async fn resolve_runs_against_a_mock_source() {
        let source = MockSource::default().with(packument(
            "left-pad",
            "1.3.0",
            "https://registry.example/left-pad-1.3.0.tgz",
        ));

        // The solver is M1-2; for now resolve is a stub, but it must accept any
        // PackageSource so it stays testable without the network.
        let resolution = resolve(&source, &[("left-pad".to_string(), "^1.0.0".to_string())])
            .await
            .unwrap();
        assert!(resolution.packages.is_empty());
    }
}
