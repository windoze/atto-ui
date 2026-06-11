#!/usr/bin/env node
// Bump the version of every published npm package in lockstep.
//
// Updates `version` in each package.json below, and rewrites any internal
// `@atto-ui/*` dependency pin (dependencies / optionalDependencies /
// peerDependencies / devDependencies) to the same version — these packages are
// released together with exact-version pins, so they must move as a group.
//
// It does NOT touch Cargo.toml: the Rust crate versions are independent and are
// not published to npm (only the built `.node` binary ships, versioned by the
// platform package.json files handled here).
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
  'crates/atto-ui-node/npm/darwin-x64/package.json',
  'crates/atto-ui-node/npm/linux-x64-gnu/package.json',
  'crates/atto-ui-node/npm/win32-x64-msvc/package.json',
]

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

let changeCount = 0

for (const relPath of PACKAGE_JSON_PATHS) {
  const absPath = join(repoRoot, relPath)
  let raw
  try {
    raw = readFileSync(absPath, 'utf8')
  } catch {
    fail(`cannot read ${relPath}`)
  }
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

  if (changes.length === 0) {
    console.log(`= ${relPath} (already ${version})`)
    continue
  }

  changeCount += changes.length
  console.log(`${dryRun ? '~' : '*'} ${relPath}`)
  for (const change of changes) console.log(`    ${change}`)

  if (!dryRun) {
    writeFileSync(absPath, `${JSON.stringify(pkg, null, indent)}\n`)
  }
}

console.log()
if (dryRun) {
  console.log(`dry run: ${changeCount} change(s) would be applied. Re-run without --dry to write.`)
} else {
  console.log(`done: applied ${changeCount} change(s) for version ${version}.`)
  console.log('Next: commit, then tag e.g.  git tag v' + version + '  && git push origin v' + version)
}
