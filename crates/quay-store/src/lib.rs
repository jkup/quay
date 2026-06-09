//! The content-addressable global store.
//!
//! Like pnpm, every package version is unpacked exactly once into a global
//! store keyed by content hash, then hard-linked into each project's
//! `node_modules`. This makes installs fast and disk-cheap across projects.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use flate2::read::GzDecoder;
use quay_core::Error;
use sha2::{Digest, Sha512};

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

    /// Verify a tarball against its `sha512-<base64>` integrity string, unpack
    /// it into the store under a content address, and return the on-disk path.
    ///
    /// The integrity is checked *before* any bytes are trusted: a mismatch
    /// returns [`Error::IntegrityMismatch`] and nothing is written to disk.
    /// Extraction is content-addressed by the verified digest, so calling this
    /// twice for the same bytes is idempotent (the second call dedupes to the
    /// already-unpacked directory without re-extracting).
    pub fn extract(&self, tarball: &[u8], integrity: &str, name: &str) -> Result<PathBuf> {
        // Verify integrity first — never unpack untrusted bytes.
        let digest = verify_integrity(tarball, integrity, name)?;

        // Content address: the verified sha512 digest, hex-encoded. Identical
        // bytes always land in the same directory, which gives us dedupe.
        let address = hex(&digest);
        let dest = self.root.join(&address);

        // Dedupe: if it's already unpacked, trust it and return.
        if dest.exists() {
            return Ok(dest);
        }

        // Unpack into a temporary sibling first, then atomically rename into
        // place. A crash mid-extraction never leaves a half-written content
        // address that a later run would mistake for complete.
        std::fs::create_dir_all(&self.root)
            .with_context(|| format!("creating store root {}", self.root.display()))?;
        let staging = self.root.join(format!(".staging-{address}"));
        if staging.exists() {
            std::fs::remove_dir_all(&staging)
                .with_context(|| format!("clearing stale staging dir {}", staging.display()))?;
        }
        std::fs::create_dir_all(&staging)
            .with_context(|| format!("creating staging dir {}", staging.display()))?;

        if let Err(e) = unpack_tgz(tarball, &staging) {
            // Best-effort cleanup so a failed unpack doesn't leak staging dirs.
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e).with_context(|| format!("unpacking tarball for `{name}`"));
        }

        match std::fs::rename(&staging, &dest) {
            Ok(()) => Ok(dest),
            // A concurrent extractor may have won the race and created `dest`
            // between our existence check and the rename. Their content is, by
            // construction, identical to ours — dedupe to it and drop staging.
            Err(_) if dest.exists() => {
                let _ = std::fs::remove_dir_all(&staging);
                Ok(dest)
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&staging);
                Err(e).with_context(|| {
                    format!("promoting {} to {}", staging.display(), dest.display())
                })
            }
        }
    }
}

/// Parse a `sha512-<base64>` integrity string, hash the tarball, and compare.
/// Returns the raw 64-byte sha512 digest on success.
fn verify_integrity(tarball: &[u8], integrity: &str, name: &str) -> Result<Vec<u8>> {
    let b64 = integrity.strip_prefix("sha512-").ok_or_else(|| {
        Error::MalformedIntegrity(
            integrity.to_string(),
            "expected `sha512-<base64>` prefix".to_string(),
        )
    })?;
    let expected = BASE64.decode(b64).map_err(|e| {
        Error::MalformedIntegrity(integrity.to_string(), format!("invalid base64: {e}"))
    })?;

    let actual = Sha512::digest(tarball);

    if actual.as_slice() != expected.as_slice() {
        return Err(Error::IntegrityMismatch {
            package: name.to_string(),
            expected: integrity.to_string(),
            actual: format!("sha512-{}", BASE64.encode(actual)),
        }
        .into());
    }

    Ok(actual.to_vec())
}

/// Gunzip + untar a `.tgz` into `dest`, stripping npm's leading `package/`
/// path component so files land at the package root.
fn unpack_tgz(tarball: &[u8], dest: &Path) -> Result<()> {
    let gz = GzDecoder::new(tarball);
    let mut archive = tar::Archive::new(gz);
    archive.set_preserve_permissions(false);

    for entry in archive.entries().context("reading tar entries")? {
        let mut entry = entry.context("reading tar entry")?;
        let path = entry
            .path()
            .context("decoding tar entry path")?
            .into_owned();

        // npm tarballs wrap everything under `package/`; strip that one
        // leading component. Entries without it are kept as-is.
        let stripped: PathBuf = path.components().skip(1).collect();
        if stripped.as_os_str().is_empty() {
            continue;
        }

        // Guard against path traversal (`..`, absolute paths) escaping `dest`.
        let out = dest.join(&stripped);
        if !out.starts_with(dest) {
            anyhow::bail!("tar entry `{}` escapes the store directory", path.display());
        }

        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }

        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&out)
                .with_context(|| format!("creating dir {}", out.display()))?;
        } else {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .with_context(|| format!("reading contents of {}", path.display()))?;
            std::fs::write(&out, &buf).with_context(|| format!("writing {}", out.display()))?;
        }
    }

    Ok(())
}

/// Lower-case hex encoding of a byte slice.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    /// Build a minimal npm-style `.tgz` in memory: a gzipped tar whose entries
    /// are prefixed with `package/`. Returns `(tarball_bytes, integrity)`.
    fn make_fixture(files: &[(&str, &[u8])]) -> (Vec<u8>, String) {
        use flate2::Compression;
        use flate2::write::GzEncoder;

        let mut tar_buf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);
            for (name, contents) in files {
                let mut header = tar::Header::new_gnu();
                header.set_size(contents.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(
                        &mut header,
                        format!("package/{name}"),
                        std::io::Cursor::new(contents),
                    )
                    .unwrap();
            }
            builder.finish().unwrap();
        }

        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(&tar_buf).unwrap();
        let tarball = gz.finish().unwrap();

        let digest = Sha512::digest(&tarball);
        let integrity = format!("sha512-{}", BASE64.encode(digest));
        (tarball, integrity)
    }

    #[test]
    fn extracts_and_strips_package_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let (tarball, integrity) = make_fixture(&[
            ("package.json", br#"{"name":"demo","version":"1.0.0"}"#),
            ("index.js", b"module.exports = 42;\n"),
        ]);

        let path = store.extract(&tarball, &integrity, "demo").unwrap();

        assert!(path.starts_with(dir.path()));
        // `package/` was stripped: files live at the package root.
        let pkg_json = std::fs::read_to_string(path.join("package.json")).unwrap();
        assert!(pkg_json.contains("\"demo\""));
        let index = std::fs::read_to_string(path.join("index.js")).unwrap();
        assert_eq!(index, "module.exports = 42;\n");
    }

    #[test]
    fn rejects_integrity_mismatch_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let (tarball, _good) = make_fixture(&[("index.js", b"hello")]);
        // A valid-format integrity for *different* bytes.
        let (_other, wrong) = make_fixture(&[("index.js", b"goodbye")]);

        let err = store.extract(&tarball, &wrong, "demo").unwrap_err();
        let typed = err.downcast_ref::<Error>().expect("typed core error");
        assert!(
            matches!(typed, Error::IntegrityMismatch { package, .. } if package == "demo"),
            "expected IntegrityMismatch, got {typed:?}"
        );

        // Nothing was unpacked.
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(entries.is_empty(), "store should be empty after rejection");
    }

    #[test]
    fn rejects_malformed_integrity() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let (tarball, _) = make_fixture(&[("index.js", b"hello")]);

        let err = store.extract(&tarball, "sha1-abc", "demo").unwrap_err();
        let typed = err.downcast_ref::<Error>().expect("typed core error");
        assert!(matches!(typed, Error::MalformedIntegrity(..)));
    }

    #[test]
    fn extracting_twice_is_idempotent_dedupe() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let (tarball, integrity) = make_fixture(&[("index.js", b"twice")]);

        let first = store.extract(&tarball, &integrity, "demo").unwrap();
        let second = store.extract(&tarball, &integrity, "demo").unwrap();

        // Same content address both times.
        assert_eq!(first, second);

        // Exactly one content-addressed directory exists (no staging leftovers).
        let dirs: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect();
        assert_eq!(dirs.len(), 1, "expected a single store entry, got {dirs:?}");
    }
}
