# Quay

**An AI-native, Rust-based alternative to npm — and eventually Node.js.**

Quay is a fast, drop-in package manager for the JavaScript ecosystem. It reads
your existing `package.json`, installs from the real npm registry, and uses a
pnpm-style content-addressable store so installs are fast and disk-cheap.

The long game: once the package manager is solid, grow into a full Node.js
alternative runtime. Package management first because the runtime needs a JS
engine — the manager is the part we can ship standalone and useful *today*.

> **Why Rust?** The modern JS toolchain renaissance (uv, Biome, Turborepo,
> Rolldown, swc) is Rust for a reason: it's fast, and its strict compiler gives
> AI coding agents a tight self-correction loop. Quay is built to be developed
> largely by agents.

## Status

🚧 Early scaffold. The crate boundaries and CLI pipeline exist and compile;
the resolver, store, and script runner are stubs. See [ROADMAP.md](./ROADMAP.md).

## Quick start

```sh
cargo run -p quay-cli -- --help
cargo run -p quay-cli -- install      # reads package.json, writes quay.lock
```

## Workspace layout

| Crate            | Responsibility                                           |
| ---------------- | -------------------------------------------------------- |
| `quay-cli`       | The `quay` binary: arg parsing + wiring (thin).          |
| `quay-core`      | Shared types: package names, ids, `package.json` model.  |
| `quay-registry`  | Async client for an npm-compatible registry.             |
| `quay-resolver`  | Version solving (PubGrub-style) → pinned graph.          |
| `quay-store`     | Content-addressable global store + `node_modules` links. |
| `quay-lock`      | `quay.lock` read/write for reproducible installs.        |

## License

MIT
