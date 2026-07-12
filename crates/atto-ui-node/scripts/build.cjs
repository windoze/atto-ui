'use strict'

const path = require('node:path')
const { spawnSync } = require('node:child_process')

const root = path.resolve(__dirname, '..')
const buildArgs = [
  'exec',
  '--yes',
  '--package=@napi-rs/cli@3.1.5',
  '--',
  'napi',
  'build',
  '--platform',
  ...process.argv.slice(2),
]

run('npm', buildArgs)
run(process.execPath, [path.join(__dirname, 'patch-generated.cjs')])

function run(command, args) {
  const result = spawnSync(command, args, {
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
}
