'use strict'

const { execSync } = require('node:child_process')
const { existsSync, readFileSync } = require('node:fs')
const { join, resolve } = require('node:path')

const loadErrors = []

// Load the napi binding from a user override, packaged binary, optional platform package,
// or the workspace crate used during local development.
function loadNative() {
  for (const override of [
    process.env.ATTO_UI_NATIVE_LIBRARY_PATH,
    process.env.NAPI_RS_NATIVE_LIBRARY_PATH,
  ]) {
    if (override) {
      const loaded = tryRequire(override)
      if (loaded) return loaded
    }
  }

  const triple = platformTriple()
  if (triple) {
    const localBinary = join(__dirname, `atto_ui_node.${triple}.node`)
    if (existsSync(localBinary)) {
      const loaded = tryRequire(localBinary)
      if (loaded) return loaded
    }

    for (const packageName of [`@atto-ui/core-${triple}`, `@atto-ui/node-${triple}`]) {
      const loaded = tryRequire(packageName)
      if (loaded) return loaded
    }
  }

  for (const request of ['@atto-ui/node', resolve(__dirname, '../../crates/atto-ui-node')]) {
    const loaded = tryRequire(request)
    if (loaded) return loaded
  }

  const error = new Error(
    `Cannot load atto-ui native binding for ${process.platform}/${process.arch}`,
  )
  error.cause = loadErrors
  throw error
}

function tryRequire(request) {
  try {
    return require(request)
  } catch (error) {
    loadErrors.push(error)
    return undefined
  }
}

function platformTriple() {
  switch (process.platform) {
    case 'darwin':
      if (process.arch === 'arm64' || process.arch === 'x64') {
        return `darwin-${process.arch}`
      }
      break
    case 'win32':
      if (process.arch === 'arm64' || process.arch === 'ia32' || process.arch === 'x64') {
        return `win32-${process.arch}-msvc`
      }
      break
    case 'linux':
      return linuxTriple()
    default:
      break
  }
  loadErrors.push(new Error(`Unsupported platform: ${process.platform}/${process.arch}`))
  return undefined
}

function linuxTriple() {
  const libc = isMusl() ? 'musl' : 'gnu'
  switch (process.arch) {
    case 'arm':
      return libc === 'musl' ? 'linux-arm-musleabihf' : 'linux-arm-gnueabihf'
    case 'arm64':
    case 'x64':
      return `linux-${process.arch}-${libc}`
    default:
      loadErrors.push(new Error(`Unsupported Linux architecture: ${process.arch}`))
      return undefined
  }
}

function isMusl() {
  if (process.platform !== 'linux') return false

  const report = typeof process.report?.getReport === 'function'
    ? process.report.getReport()
    : undefined
  if (report?.header?.glibcVersionRuntime) return false
  if (Array.isArray(report?.sharedObjects) && report.sharedObjects.some(isMuslPath)) {
    return true
  }

  try {
    return readFileSync('/usr/bin/ldd', 'utf8').includes('musl')
  } catch {
    try {
      return execSync('ldd --version', { encoding: 'utf8' }).includes('musl')
    } catch {
      return false
    }
  }
}

function isMuslPath(path) {
  return path.includes('libc.musl-') || path.includes('ld-musl-')
}

module.exports = loadNative()
