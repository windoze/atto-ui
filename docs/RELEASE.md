# CI And Release

## Continuous Integration

`.github/workflows/ci.yml` runs on pull requests, pushes to `main`/`master`, and manual dispatch. `.github/workflows/release.yml` repeats the same Linux test gate before any tag-based native build or publish job.

Coverage:

| Step | Command or coverage |
|---|---|
| Rust formatting | `cargo fmt --all -- --check` |
| Rust lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| Rust tests | `cargo test --all --all-targets` |
| Native binding | `npm run build --prefix crates/atto-ui-node` |
| Node binding tests | `npm test --prefix crates/atto-ui-node` |
| Core package | `tsc` typecheck and `npm test --prefix packages/core` |
| React package | `npm run typecheck --prefix packages/react` and `npm test --prefix packages/react` |
| Runtime compatibility | `npm run test:runtime:bun --prefix packages/core` and `npm run test:runtime:deno --prefix packages/core` |
| Packaging | `npm pack --dry-run --json` for native, platform, core, and React packages |

The React test suite includes reconciler matrix tests, headless integration, PTY tests, and e2e coverage.

## Workspace App Scope

`crates/atto-agent-app` is a workspace-only TUI application used for local agent development, deterministic PTY fixtures, and manual DeepSeek smoke validation. It is not part of the npm package set, is not published to crates.io, and is marked `publish = false` in its `Cargo.toml` to prevent accidental `cargo publish`.

Release tags still validate `atto-agent-app` through the workspace Rust fmt, clippy, and test gates. The ignored real DeepSeek smoke test remains manual because it requires `DEEPSEEK_API_KEY` and external network access.

## Local Preflight

Run these before creating a release tag:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --all --all-targets
npm ci --prefix packages/react --ignore-scripts
npm run build --prefix crates/atto-ui-node
npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit
npm test --prefix crates/atto-ui-node
npm test --prefix packages/core
npm run test:runtime:bun --prefix packages/core
npm run test:runtime:deno --prefix packages/core
npm run typecheck --prefix packages/react
npm test --prefix packages/react
```

If Bun is not installed locally, use the official npm binary package for the smoke:

```sh
npm exec --yes --package=bun@1.3.14 -- npm run test:runtime:bun --prefix packages/core
```

## Tag Release

`.github/workflows/release.yml` runs on `v*` tags and manual dispatch. Manual dispatch performs the test gate, build, and pack verification without publishing. Tag pushes publish only after the test gate, native builds, and pack verification pass, and when `secrets.NPM_TOKEN` is configured.

Build matrix:

| Runner | Rust target | npm platform package |
|---|---|---|
| `macos-14` | `aarch64-apple-darwin` | `@atto-ui/node-darwin-arm64` |
| `macos-13` | `x86_64-apple-darwin` | `@atto-ui/node-darwin-x64` |
| `ubuntu-22.04` | `x86_64-unknown-linux-gnu` | `@atto-ui/node-linux-x64-gnu` |
| `windows-2022` | `x86_64-pc-windows-msvc` | `@atto-ui/node-win32-x64-msvc` |

Publish order:

1. Platform binary packages under `crates/atto-ui-node/npm/*`.
2. `@atto-ui/node`.
3. `@atto-ui/core`.
4. `@atto-ui/react`.

The release workflow first runs the Rust/JS/runtime compatibility gate, then builds native artifacts, downloads them, runs `npm run npm:artifacts --prefix crates/atto-ui-node`, verifies every package with `npm pack --dry-run --json`, then publishes.

## Creating A Release

```sh
git tag v0.1.0
git push origin v0.1.0
```

After the workflow completes, verify that installing `@atto-ui/core` on each supported platform selects the matching optional native package and can run `require('@atto-ui/core').version()` without a local Rust toolchain.
