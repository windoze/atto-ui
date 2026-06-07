# CI And Release

## Continuous Integration

`.github/workflows/ci.yml` runs on pull requests, pushes to `main`/`master`, and manual dispatch.

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
| Runtime compatibility | `npm run test:runtime:bun` and `npm run test:runtime:deno` |
| Packaging | `npm pack --dry-run --json` for native, platform, core, and React packages |

The React test suite includes reconciler matrix tests, headless integration, PTY tests, and e2e coverage.

## Local Preflight

Run these before creating a release tag:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --all --all-targets
npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform
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

`.github/workflows/release.yml` runs on `v*` tags and manual dispatch. Manual dispatch performs the build and pack verification without publishing. Tag pushes publish when `secrets.NPM_TOKEN` is configured.

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

The release workflow downloads all native artifacts, runs `napi artifacts --npm-dir npm --output-dir .`, verifies every package with `npm pack --dry-run --json`, then publishes.

## Creating A Release

```sh
git tag v0.1.0
git push origin v0.1.0
```

After the workflow completes, verify that installing `@atto-ui/core` on each supported platform selects the matching optional native package and can run `require('@atto-ui/core').version()` without a local Rust toolchain.
