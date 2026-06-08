# CLAUDE.md — working agreement for agents on Quay

Quay is an AI-native, Rust-based alternative to npm (and eventually Node.js).
Most of this codebase is meant to be written by AI agents. Read this before
making changes, then read [ROADMAP.md](./ROADMAP.md) for what to work on.

## The prime directive

**Keep `main` green.** After every change, the following must pass:

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

If you can't make all four pass, leave the work behind a stub that compiles
(return an `anyhow::bail!` with a `TODO(quay) Mx` note) rather than breaking the
build. A green baseline is what lets the next agent move fast.

## Architecture rules

- **The CLI is thin.** `quay-cli` only parses args, wires crates together, and
  prints. All real logic lives in library crates so it's testable and reusable.
- **Respect crate boundaries.** Dependencies flow one way:
  `cli → {lock → resolver → registry → core}` and `cli → store → core`.
  `quay-core` depends on nothing else in the workspace. Don't add a back-edge.
- **npm-compatible by default.** We read `package.json` and install from
  registry.npmjs.org so existing projects work unchanged. Don't invent a new
  manifest format; extend the existing one.
- **Network/FS only in their crates.** `quay-registry` owns HTTP, `quay-store`
  owns the filesystem store, `quay-lock` owns lockfile IO. `quay-core` and
  `quay-resolver` stay pure and easily unit-testable.

## Conventions

- Edition 2024, toolchain pinned in `rust-toolchain.toml`. Shared dep versions
  live in the root `[workspace.dependencies]`; reference them with
  `dep.workspace = true` rather than per-crate version strings.
- Errors: `quay-core` defines a typed `Error`; library crates that touch IO use
  `anyhow::Result` at their edges. The CLI returns `anyhow::Result` from `main`.
- Mark unfinished work with `TODO(quay) Mx:` referencing the roadmap milestone.
- Add a unit test alongside any non-trivial logic. Prefer pure functions that
  are testable without network/FS; mock the registry where needed.

## Where things are

- `crates/quay-core` — types: `PackageName`, `PackageId`, `Manifest`, `Error`.
- `crates/quay-registry` — `RegistryClient`, `Packument`.
- `crates/quay-resolver` — `resolve()` → `Resolution` (currently a stub).
- `crates/quay-store` — `Store` (content-addressable; extraction is a stub).
- `crates/quay-lock` — `Lockfile` read/write.
- `crates/quay-cli` — `quay install | add | run`.
