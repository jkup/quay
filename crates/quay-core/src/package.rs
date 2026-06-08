//! Package identity: names (incl. scoped `@scope/name`) and resolved ids.

use std::fmt;

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// A validated npm-compatible package name, e.g. `lodash` or `@types/node`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageName(String);

impl PackageName {
    pub fn parse(s: impl Into<String>) -> Result<Self> {
        let s = s.into();
        // Minimal validation for now; see ROADMAP for full npm name rules.
        if s.is_empty() || s.len() > 214 {
            return Err(Error::InvalidPackageName(s));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `true` for `@scope/name` style names.
    pub fn is_scoped(&self) -> bool {
        self.0.starts_with('@')
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A fully resolved package: a name pinned to an exact version.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageId {
    pub name: PackageName,
    pub version: Version,
}

impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}
