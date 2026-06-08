//! Dependency resolution: turn a set of version *requirements* into an exact,
//! conflict-free set of package versions (the resolution graph).
//!
//! The intended algorithm is PubGrub-style version solving (the same family
//! pnpm/cargo/uv use) for fast, high-quality error messages on conflicts.
//! This module currently exposes the shape; the solver itself is the first
//! big task for the agent — see ROADMAP.md, milestone M1.

use std::collections::BTreeMap;

use quay_core::PackageId;
use quay_registry::RegistryClient;

/// A resolved dependency graph: every package pinned to one exact version.
#[derive(Debug, Default, Clone)]
pub struct Resolution {
    pub packages: BTreeMap<String, PackageId>,
}

/// Resolve a manifest's direct dependencies (and, eventually, transitive ones)
/// into a fully pinned [`Resolution`].
///
/// `direct` is `(name, version_req)` straight from package.json.
pub async fn resolve(
    _registry: &RegistryClient,
    direct: &[(String, String)],
) -> anyhow::Result<Resolution> {
    // TODO(quay) M1: implement PubGrub solving over registry packuments.
    // For now we just record what was asked for so the CLI pipeline runs
    // end-to-end and the agent has a green baseline to build on.
    tracing::warn!("resolver is a stub: returning empty resolution");
    let _ = direct;
    Ok(Resolution::default())
}
