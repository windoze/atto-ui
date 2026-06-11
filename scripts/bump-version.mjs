#!/usr/bin/env node
// Bump the release version across every published artifact in lockstep:
//
//   * npm packages   — `version` in each package.json below, plus any internal
//     `@atto-ui/*` dependency pin (dependencies / optionalDependencies /
//     peerDependencies / devDependencies), which are released together with
//     exact-version pins and must move as a group.
//   * Rust crates    — `[package] version` in the root Cargo.toml and every
//     workspace member (auto-discovered from `[workspace] members`). Internal
//     `atto-ui-*` deps are plain `path` dependencies with no version pin, so
//     there is nothing else to rewrite on the Cargo side.
//   * Python package — `[project] version` in the maturin pyproject.toml.
//
// After bumping Cargo.toml versions, refresh Cargo.lock (e.g. `cargo build`)
// and include it in the release commit.
//
// Usage:
//   node scripts/bump-version.mjs <version> [--dry]
//   node scripts/bump-version.mjs 0.2.0
//   node scripts/bump-version.mjs 0.2.0 --dry   # preview, write nothing

import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '..')

// Every publishable npm package, relative to the repo root.
const PACKAGE_JSON_PATHS = [
  'packages/core/package.json',
  'packages/react/package.json',
  'crates/atto-ui-node/package.json',
  'crates/atto-ui-node/npm/darwin-arm64/package.json',
  'crates/atto-ui-node/npm/linux-x64-gnu/package.json',
  'crates/atto-ui-node/npm/win32-x64-msvc/package.json',
]

// maturin-built Python packages, relative to the repo root.
const PYPROJECT_PATHS = ['crates/atto-ui-python/pyproject.toml']

const DEP_FIELDS = ['dependencies', 'optionalDependencies', 'peerDependencies', 'devDependencies']
// Dependency values that are not plain versions and must be left untouched.
const NON_VERSION_PREFIXES = ['workspace:', 'file:', 'link:', 'npm:', 'git', 'http']

function fail(message) {
  console.error(`error: ${message}`)
  process.exit(1)
}

const args = process.argv.slice(2)
const dryRun = args.includes('--dry')
const version = args.find((a) => !a.startsWith('-'))

if (!version) {
  fail('missing <version>. usage: node scripts/bump-version.mjs <version> [--dry]')
}
// Permissive semver: x.y.z with optional -prerelease / +build.
if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)) {
  fail(`"${version}" is not a valid semver version (expected e.g. 1.2.3 or 1.2.3-rc.1)`)
}

function detectIndent(raw) {
  const match = raw.match(/\n(\s+)"/)
  return match ? match[1] : '  '
}

function read(relPath) {
  try {
    return readFileSync(join(repoRoot, relPath), 'utf8')
  } catch {
    fail(`cannot read ${relPath}`)
  }
}

let changeCount = 0

function report(relPath, changes, write) {
  if (changes.length === 0) {
    console.log(`= ${relPath} (already ${version})`)
    return
  }
  changeCount += changes.length
  console.log(`${dryRun ? '~' : '*'} ${relPath}`)
  for (const change of changes) console.log(`    ${change}`)
  if (!dryRun) write()
}

// Resolve the root Cargo.toml plus every workspace member's Cargo.toml.
function cargoTomlPaths() {
  const raw = read('Cargo.toml')
  const wsIdx = raw.indexOf('[workspace]')
  const slice = wsIdx >= 0 ? raw.slice(wsIdx) : ''
  const membersMatch = slice.match(/members\s*=\s*\[([\s\S]*?)\]/)
  const members = membersMatch
    ? [...membersMatch[1].matchAll(/"([^"]+)"/g)].map((m) => m[1])
    : []
  return ['Cargo.toml', ...members.map((dir) => `${dir}/Cargo.toml`)]
}

function bumpPackageJson(relPath) {
  const raw = read(relPath)
  const pkg = JSON.parse(raw)
  const indent = detectIndent(raw)
  const changes = []

  if (pkg.version !== version) {
    changes.push(`version ${pkg.version} -> ${version}`)
    pkg.version = version
  }

  for (const field of DEP_FIELDS) {
    const deps = pkg[field]
    if (!deps || typeof deps !== 'object') continue
    for (const [name, range] of Object.entries(deps)) {
      if (!name.startsWith('@atto-ui/')) continue
      if (typeof range !== 'string') continue
      if (NON_VERSION_PREFIXES.some((p) => range.startsWith(p))) continue
      if (range === version) continue
      changes.push(`${field}.${name} ${range} -> ${version}`)
      deps[name] = version
    }
  }

  report(relPath, changes, () =>
    writeFileSync(join(repoRoot, relPath), `${JSON.stringify(pkg, null, indent)}\n`),
  )
}

// Replace the first `version = "..."` inside a given TOML table (e.g. `package`
// or `project`), preserving all formatting and comments.
function bumpTomlVersion(relPath, section) {
  const raw = read(relPath)
  const lines = raw.split('\n')
  const changes = []
  let inSection = false
  let done = false

  for (let i = 0; i < lines.length; i++) {
    const header = lines[i].match(/^\s*\[([^\]]+)\]/)
    if (header) {
      inSection = header[1].trim() === section
      continue
    }
    if (inSection && !done) {
      const m = lines[i].match(/^(\s*version\s*=\s*")([^"]*)(".*)$/)
      if (m) {
        if (m[2] !== version) {
          changes.push(`[${section}] version ${m[2]} -> ${version}`)
          lines[i] = `${m[1]}${version}${m[3]}`
        }
        done = true
      }
    }
  }

  if (!done) {
    fail(`no [${section}] version field found in ${relPath}`)
  }

  report(relPath, changes, () => writeFileSync(join(repoRoot, relPath), lines.join('\n')))
}

for (const relPath of PACKAGE_JSON_PATHS) bumpPackageJson(relPath)
for (const relPath of cargoTomlPaths()) bumpTomlVersion(relPath, 'package')
for (const relPath of PYPROJECT_PATHS) bumpTomlVersion(relPath, 'project')

console.log()
if (dryRun) {
  console.log(`dry run: ${changeCount} change(s) would be applied. Re-run without --dry to write.`)
} else {
  console.log(`done: applied ${changeCount} change(s) for version ${version}.`)
  console.log('Next: refresh Cargo.lock (cargo build), commit, then tag e.g.')
  console.log(`  git tag v${version} && git push origin v${version}`)
}
