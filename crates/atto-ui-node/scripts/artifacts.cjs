'use strict'

const fs = require('node:fs')
const path = require('node:path')
const { spawnSync } = require('node:child_process')

const root = path.resolve(__dirname, '..')
const pkg = require(path.join(root, 'package.json'))
const binaryName = pkg.napi?.binaryName ?? 'index'
const npmDir = path.join(root, 'npm')

for (const entry of fs.readdirSync(root)) {
  const prefix = `${binaryName}.`
  const suffix = '.node'
  if (!entry.startsWith(prefix) || !entry.endsWith(suffix)) {
    continue
  }
  const platform = entry.slice(prefix.length, -suffix.length)
  if (!fs.existsSync(path.join(npmDir, platform))) {
    fs.rmSync(path.join(root, entry), { force: true })
  }
}

const args = [
  'exec',
  '--yes',
  '--package=@napi-rs/cli@3.1.5',
  '--',
  'napi',
  'artifacts',
  '--npm-dir',
  'npm',
  '--output-dir',
  '.',
]
const result = spawnSync('npm', args, {
  cwd: root,
  stdio: 'inherit',
  shell: process.platform === 'win32',
})
if (result.error) {
  throw result.error
}
if (result.status !== 0) {
  process.exit(result.status ?? 1)
}
