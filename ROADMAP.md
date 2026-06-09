# Quay Roadmap

Milestones are ordered so each builds on a green, working previous one. An agent
should pick the lowest-numbered unfinished task, implement it end-to-end (code +
tests + docs), keep the four `main`-green checks passing (see CLAUDE.md), then
move on. When a task lands, tick its box in the same change so this file always
reflects reality.

**Legend:** Tasks marked **(parallel-safe)** have no ordering dependency on their
siblings and can be worked concurrently.

## M0 — Scaffold ✅ (done)

- [x] Cargo workspace with six crates and clean dependency boundaries.
- [x] `package.json` parsing (`quay-core::Manifest`).
- [x] npm registry client shape (`quay-registry`).
- [x] CLI pipeline: `quay install` reads manifest → resolver → writes lockfile.

## M1 — Real dependency resolution

The headline feature. Turn version requirements into an exact, pinned graph.

- [x] Fetch packuments for direct deps via `RegistryClient`.
- [x] Implement PubGrub-style version solving over the registry, including
      transitive dependencies. (Consider the `pubgrub` crate vs. hand-rolling.)
- [ ] Carry tarball URL + integrity through `Resolution` into `Lockfile`.
- [x] Helpful conflict error messages ("a needs b@^1, c needs b@^2").
- [x] Unit tests with a mocked registry covering: simple tree, shared transitive
      dep, version conflict, cyclic deps.

## M2 — Install to disk (content-addressable store)

- [ ] `Store::extract`: verify integrity, unpack `.tgz`, dedupe by content hash.
- [ ] Link resolved packages into `./node_modules` (hard links from the store).
- [ ] Reproduce installs from an existing `quay.lock` without re-resolving.
- [ ] Parallel downloads with a bounded concurrency limit.
- [ ] Integration test: install `is-odd` (or another tiny, stable package)
      end-to-end.

## M3 — Manager UX parity with npm

- [ ] `quay add <pkg>` — resolve, update `package.json`, install.
- [ ] `quay remove <pkg>`. **(parallel-safe)**
- [ ] `quay run <script>` — execute via shell with `node_modules/.bin` on PATH.
      **(parallel-safe)**
- [ ] Workspaces / monorepo support (`workspaces` field in package.json).
- [ ] `quay install --frozen-lockfile` for CI. **(parallel-safe)**

## M4 — Speed & polish

- [ ] Benchmark cold/warm installs against npm + pnpm; target warm installs
      within 2× of pnpm; publish the numbers.
- [ ] Metadata cache to avoid refetching packuments. **(parallel-safe)**
- [ ] Pretty progress output; `--json` machine-readable mode. **(parallel-safe)**

## M5 — Toward a runtime (research)

This is the "Node.js alternative" north star. It needs a JS engine.

**Guardrail:** M5 tasks are research spikes. Timebox each to a single spike
branch, write up findings, and STOP — do not begin M5 *implementation* (engine
integration, Node API surface) without explicit human sign-off.

- [ ] Spike: embed a JS engine (V8 via `rusty_v8`, or QuickJS via `rquickjs`).
- [ ] Minimal CommonJS/ESM module loader resolving against `node_modules`.
- [ ] Implement a starter slice of the Node API surface (fs, path, process).
- [ ] `quay run script.js` executes JS directly — no Node required.
