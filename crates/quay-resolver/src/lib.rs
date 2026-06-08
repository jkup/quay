//! Dependency resolution: turn a set of version *requirements* into an exact,
//! conflict-free set of package versions (the resolution graph).
//!
//! The algorithm is PubGrub-style version solving (the same family pnpm/cargo/uv
//! use): we accumulate constraints per package, always try the highest version
//! that satisfies every constraint, and backtrack when a choice paints a later
//! package into a corner. When no version can satisfy a package's accumulated
//! requirements we surface a [`Conflict`] that names the conflicting requesters.
//!
//! The solver is split into two phases so the search itself stays pure and
//! synchronous (and therefore trivially unit-testable):
//!
//! 1. **Fetch** — walk the dependency graph from the direct deps and pull every
//!    reachable [`Packument`] into an in-memory cache. This is the only phase
//!    that touches the [`PackageSource`] (network or mock).
//! 2. **Solve** — backtracking search over the cached packuments. No IO.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use quay_core::{PackageId, PackageName};
use quay_registry::{PackageSource, Packument, VersionMeta};
use semver::{Version, VersionReq};

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

/// Sentinel requester used for the top-level dependencies pulled straight from
/// the manifest (they have no parent package).
const ROOT: &str = "(root)";

/// One requirement placed on a package: who asked for it, and the raw semver
/// range they asked for. Kept around so conflict messages can name names.
#[derive(Debug, Clone)]
struct Constraint {
    /// The package (and version) that introduced this requirement, or [`ROOT`]
    /// for a direct/manifest dependency.
    requester: String,
    /// The original requirement string, e.g. `^1.0.0`, preserved for messages.
    raw: String,
    /// The parsed range used for matching.
    req: VersionReq,
}

/// A resolution failure: no single version of `package` satisfies every
/// requirement placed on it. The `requirements` name each requester and the
/// range it demanded, e.g. `a@1.0.0 needs ^1.0.0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub package: String,
    pub requirements: Vec<ConflictingRequirement>,
}

/// One side of a [`Conflict`]: a requester and the range it demanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictingRequirement {
    pub requester: String,
    pub range: String,
}

impl fmt::Display for Conflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "no version of `{}` satisfies all requirements: ",
            self.package
        )?;
        for (i, r) in self.requirements.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{} needs {}", r.requester, r.range)?;
        }
        Ok(())
    }
}

impl std::error::Error for Conflict {}

/// Resolve a manifest's direct dependencies (and their transitive deps) into a
/// fully pinned [`Resolution`].
///
/// `direct` is `(name, version_req)` straight from package.json. The package
/// metadata comes from any [`PackageSource`], so the resolver can be unit
/// tested against an in-memory mock instead of the live network.
pub async fn resolve<S: PackageSource>(
    source: &S,
    direct: &[(String, String)],
) -> anyhow::Result<Resolution> {
    // Parse the direct requirements up front so a bad range fails fast and
    // clearly, before we touch the network.
    let mut roots: Vec<Constraint> = Vec::with_capacity(direct.len());
    let mut root_names: Vec<String> = Vec::with_capacity(direct.len());
    for (name, raw) in direct {
        roots.push(Constraint {
            requester: ROOT.to_string(),
            raw: raw.clone(),
            req: parse_req(raw)?,
        });
        root_names.push(name.clone());
    }

    // Phase 1: fetch every reachable packument into a cache.
    let cache = fetch_all(source, &root_names).await?;

    // Phase 2: pure backtracking search over the cache.
    let mut initial: BTreeMap<String, Vec<Constraint>> = BTreeMap::new();
    for (name, c) in root_names.into_iter().zip(roots) {
        initial.entry(name).or_default().push(c);
    }

    let solver = Solver { cache: &cache };
    let selected = solver
        .solve(initial, BTreeMap::new())
        .map_err(anyhow::Error::new)?;

    // Materialise the pinned graph into ResolvedPackages.
    let mut packages = BTreeMap::new();
    for (name, version) in selected {
        let meta = solver
            .version_meta(&name, &version)
            .expect("solver only selects versions present in the cache");
        let id = PackageId {
            name: PackageName::parse(name.clone())?,
            version,
        };
        packages.insert(
            name,
            ResolvedPackage {
                id,
                tarball: meta.dist.tarball.clone(),
                integrity: meta.dist.integrity.clone(),
            },
        );
    }

    Ok(Resolution { packages })
}

/// Parse a requirement string, treating an empty string (and `latest`/`*`,
/// which npm uses for "any version") as "match anything".
fn parse_req(raw: &str) -> anyhow::Result<VersionReq> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "latest" || trimmed == "*" {
        return Ok(VersionReq::STAR);
    }
    VersionReq::parse(trimmed)
        .map_err(|e| anyhow::anyhow!("invalid version requirement `{raw}`: {e}"))
}

/// Walk the dependency graph breadth-first from `roots`, downloading each
/// reachable packument exactly once. The visited set makes this terminate even
/// when packages depend on each other in a cycle.
async fn fetch_all<S: PackageSource>(
    source: &S,
    roots: &[String],
) -> anyhow::Result<BTreeMap<String, Packument>> {
    let mut cache: BTreeMap<String, Packument> = BTreeMap::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    for name in roots {
        if seen.insert(name.clone()) {
            queue.push_back(name.clone());
        }
    }

    while let Some(name) = queue.pop_front() {
        let packument = source.packument(&name).await?;
        // Enqueue every dependency named by any version; we over-fetch slightly
        // (deps of versions we may never pick) to keep the solver phase pure.
        for meta in packument.versions.values() {
            for dep_name in meta.dependencies.keys() {
                if seen.insert(dep_name.clone()) {
                    queue.push_back(dep_name.clone());
                }
            }
        }
        cache.insert(name, packument);
    }

    Ok(cache)
}

/// Pure backtracking solver over a fixed set of cached packuments.
struct Solver<'a> {
    cache: &'a BTreeMap<String, Packument>,
}

impl Solver<'_> {
    /// Backtracking search.
    ///
    /// `constraints` maps each package to every requirement seen on it so far;
    /// `assignment` maps each already-chosen package to its version. Returns the
    /// completed assignment, or the first [`Conflict`] that makes the current
    /// branch unsatisfiable.
    fn solve(
        &self,
        constraints: BTreeMap<String, Vec<Constraint>>,
        assignment: BTreeMap<String, Version>,
    ) -> Result<BTreeMap<String, Version>, Conflict> {
        // 1. Any already-assigned package must still satisfy every constraint on
        //    it; a newly-added dep constraint may have invalidated an earlier
        //    pick, which fails this branch.
        for (name, version) in &assignment {
            if let Some(cs) = constraints.get(name) {
                for c in cs {
                    if !c.req.matches(version) {
                        return Err(self.conflict(name, cs));
                    }
                }
            }
        }

        // 2. Pick the next unassigned, constrained package (deterministic order
        //    via BTreeMap keeps the search reproducible).
        let next = constraints
            .keys()
            .find(|name| !assignment.contains_key(*name));
        let Some(name) = next.cloned() else {
            // Everything constrained is assigned and consistent: done.
            return Ok(assignment);
        };

        let cs = &constraints[&name];

        // 3. Candidate versions: those satisfying *every* current constraint,
        //    highest first (npm/cargo prefer the newest compatible release).
        let candidates = self.candidates(&name, cs);
        if candidates.is_empty() {
            return Err(self.conflict(&name, cs));
        }

        // 4. Try each candidate, layering in its dependencies, and recurse.
        let mut last_err = self.conflict(&name, cs);
        for version in candidates {
            let mut next_constraints = constraints.clone();
            let meta = self
                .version_meta(&name, &version)
                .expect("candidate came from the cache");
            let requester = format!("{name}@{version}");
            for (dep_name, dep_raw) in &meta.dependencies {
                // A dep range that won't even parse can't be satisfied; treat it
                // as this candidate failing rather than aborting the whole solve.
                let Ok(req) = parse_req(dep_raw) else {
                    continue;
                };
                next_constraints
                    .entry(dep_name.clone())
                    .or_default()
                    .push(Constraint {
                        requester: requester.clone(),
                        raw: dep_raw.clone(),
                        req,
                    });
            }

            let mut next_assignment = assignment.clone();
            next_assignment.insert(name.clone(), version);

            match self.solve(next_constraints, next_assignment) {
                Ok(done) => return Ok(done),
                Err(e) => last_err = e,
            }
        }

        Err(last_err)
    }

    /// Versions of `name` that satisfy every constraint, sorted high → low.
    fn candidates(&self, name: &str, constraints: &[Constraint]) -> Vec<Version> {
        let Some(packument) = self.cache.get(name) else {
            return Vec::new();
        };
        let mut versions: Vec<Version> = packument
            .versions
            .keys()
            .filter_map(|v| Version::parse(v).ok())
            .filter(|v| constraints.iter().all(|c| c.req.matches(v)))
            .collect();
        versions.sort();
        versions.reverse();
        versions
    }

    /// Look up the metadata for a specific pinned version.
    fn version_meta(&self, name: &str, version: &Version) -> Option<&VersionMeta> {
        let packument = self.cache.get(name)?;
        // Match on the parsed version so `1.2.0` and `1.2.0` compare equal even
        // if the registry key has odd formatting.
        packument
            .versions
            .iter()
            .find(|(k, _)| Version::parse(k).ok().as_ref() == Some(version))
            .map(|(_, meta)| meta)
    }

    /// Build a [`Conflict`] describing why `name` cannot be satisfied.
    fn conflict(&self, name: &str, constraints: &[Constraint]) -> Conflict {
        Conflict {
            package: name.to_string(),
            requirements: constraints
                .iter()
                .map(|c| ConflictingRequirement {
                    requester: c.requester.clone(),
                    range: c.raw.clone(),
                })
                .collect(),
        }
    }
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

    /// Build a packument with one or more versions. Each `version` is
    /// `(version, &[(dep_name, dep_req)])`.
    fn packument(name: &str, versions: &[(&str, &[(&str, &str)])]) -> Packument {
        let mut map = BTreeMap::new();
        for (version, deps) in versions {
            let mut dependencies = BTreeMap::new();
            for (dn, dr) in *deps {
                dependencies.insert((*dn).to_string(), (*dr).to_string());
            }
            map.insert(
                (*version).to_string(),
                VersionMeta {
                    version: (*version).to_string(),
                    dependencies,
                    dist: Dist {
                        tarball: format!("https://registry.example/{name}-{version}.tgz"),
                        integrity: Some(format!("sha512-{name}-{version}")),
                    },
                },
            );
        }
        Packument {
            name: name.to_string(),
            versions: map,
        }
    }

    /// Convenience: assert a package resolved to an exact version.
    fn assert_pinned(res: &Resolution, name: &str, version: &str) {
        let pkg = res
            .packages
            .get(name)
            .unwrap_or_else(|| panic!("`{name}` should be in the resolution"));
        assert_eq!(pkg.id.version.to_string(), version, "version of {name}");
        assert_eq!(pkg.id.name.as_str(), name);
        // The enriched model must carry tarball + integrity through.
        assert_eq!(
            pkg.tarball,
            format!("https://registry.example/{name}-{version}.tgz")
        );
        assert_eq!(
            pkg.integrity.as_deref(),
            Some(&*format!("sha512-{name}-{version}"))
        );
    }

    #[tokio::test]
    async fn mock_source_serves_canned_packuments() {
        let source = MockSource::default().with(packument("left-pad", &[("1.3.0", &[])]));

        let pkg = source.packument("left-pad").await.unwrap();
        assert_eq!(pkg.name, "left-pad");
        assert!(pkg.versions.contains_key("1.3.0"));

        // Unknown packages error rather than hitting the network.
        assert!(source.packument("does-not-exist").await.is_err());
    }

    /// Picks the highest version satisfying a caret range, and threads the
    /// tarball + integrity from the chosen version into the resolution.
    #[tokio::test]
    async fn resolves_a_single_direct_dep() {
        let source = MockSource::default().with(packument(
            "left-pad",
            &[("1.2.0", &[]), ("1.3.0", &[]), ("2.0.0", &[])],
        ));

        let res = resolve(&source, &[("left-pad".into(), "^1.0.0".into())])
            .await
            .unwrap();

        assert_eq!(res.packages.len(), 1);
        assert_pinned(&res, "left-pad", "1.3.0");
    }

    /// A simple tree: root → a → b. All three end up pinned.
    #[tokio::test]
    async fn resolves_a_simple_transitive_tree() {
        let source = MockSource::default()
            .with(packument("a", &[("1.0.0", &[("b", "^1.0.0")])]))
            .with(packument("b", &[("1.0.0", &[("c", "^1.0.0")])]))
            .with(packument("c", &[("1.5.0", &[])]));

        let res = resolve(&source, &[("a".into(), "^1.0.0".into())])
            .await
            .unwrap();

        assert_eq!(res.packages.len(), 3);
        assert_pinned(&res, "a", "1.0.0");
        assert_pinned(&res, "b", "1.0.0");
        assert_pinned(&res, "c", "1.5.0");
    }

    /// A shared transitive dep (a→c, b→c) is deduped to a single version that
    /// satisfies both requirers.
    #[tokio::test]
    async fn shares_a_transitive_dep_across_requirers() {
        let source = MockSource::default()
            .with(packument("a", &[("1.0.0", &[("shared", "^1.0.0")])]))
            .with(packument("b", &[("1.0.0", &[("shared", ">=1.1.0")])]))
            .with(packument(
                "shared",
                &[("1.0.0", &[]), ("1.2.0", &[]), ("1.5.0", &[])],
            ));

        let res = resolve(
            &source,
            &[("a".into(), "^1.0.0".into()), ("b".into(), "^1.0.0".into())],
        )
        .await
        .unwrap();

        assert_eq!(res.packages.len(), 3);
        // Highest version satisfying both ^1.0.0 and >=1.1.0.
        assert_pinned(&res, "shared", "1.5.0");
    }

    /// Two requirers demand incompatible majors of the same package: a readable
    /// conflict naming both requirements.
    #[tokio::test]
    async fn reports_a_version_conflict() {
        let source = MockSource::default()
            .with(packument("a", &[("1.0.0", &[("b", "^1.0.0")])]))
            .with(packument("c", &[("1.0.0", &[("b", "^2.0.0")])]))
            .with(packument("b", &[("1.0.0", &[]), ("2.0.0", &[])]));

        let err = resolve(
            &source,
            &[("a".into(), "^1.0.0".into()), ("c".into(), "^1.0.0".into())],
        )
        .await
        .unwrap_err();

        let conflict = err
            .downcast_ref::<Conflict>()
            .expect("a version conflict should surface as a Conflict");
        assert_eq!(conflict.package, "b");

        // Both sides of the conflict are named, with their requesters.
        let msg = conflict.to_string();
        assert!(msg.contains("`b`"), "names the package: {msg}");
        assert!(
            msg.contains("a@1.0.0 needs ^1.0.0"),
            "names side one: {msg}"
        );
        assert!(
            msg.contains("c@1.0.0 needs ^2.0.0"),
            "names side two: {msg}"
        );
    }

    /// Cyclic dependencies (a ↔ b) resolve without looping forever.
    #[tokio::test]
    async fn resolves_a_dependency_cycle() {
        let source = MockSource::default()
            .with(packument("a", &[("1.0.0", &[("b", "^1.0.0")])]))
            .with(packument("b", &[("1.0.0", &[("a", "^1.0.0")])]));

        let res = resolve(&source, &[("a".into(), "^1.0.0".into())])
            .await
            .unwrap();

        assert_eq!(res.packages.len(), 2);
        assert_pinned(&res, "a", "1.0.0");
        assert_pinned(&res, "b", "1.0.0");
    }

    /// Backtracking: the newest version of a top-level dep is incompatible with
    /// a sibling's requirement, so the solver must fall back to an older one.
    #[tokio::test]
    async fn backtracks_to_an_older_compatible_version() {
        // app depends on a (any) and on b@1.0.0.
        // a@2.0.0 needs b@^2 (incompatible), a@1.0.0 needs b@^1 (ok).
        // The solver should reject a@2.0.0 and settle on a@1.0.0.
        let source = MockSource::default()
            .with(packument(
                "a",
                &[("1.0.0", &[("b", "^1.0.0")]), ("2.0.0", &[("b", "^2.0.0")])],
            ))
            .with(packument("b", &[("1.0.0", &[]), ("2.0.0", &[])]));

        let res = resolve(
            &source,
            &[("a".into(), "*".into()), ("b".into(), "1.0.0".into())],
        )
        .await
        .unwrap();

        assert_pinned(&res, "a", "1.0.0");
        assert_pinned(&res, "b", "1.0.0");
    }

    #[tokio::test]
    async fn rejects_an_unparseable_requirement() {
        let source = MockSource::default().with(packument("a", &[("1.0.0", &[])]));
        let err = resolve(&source, &[("a".into(), "not-a-range".into())])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid version requirement"));
    }
}
