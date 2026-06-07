const assert = require('node:assert/strict')
const { spawnSync } = require('node:child_process')
const { join } = require('node:path')
const React = require('react')
const { render } = require('../dist')
const { E2eApp } = require('./e2e_app.cjs')

function findNodeByText(node, text) {
  if (node.text === text) return node
  for (const child of node.children ?? []) {
    const found = findNodeByText(child, text)
    if (found) return found
  }
  return undefined
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

async function waitFor(predicate, label) {
  const deadline = Date.now() + 1500
  while (Date.now() < deadline) {
    const result = predicate()
    if (result) return result
    await delay(10)
  }
  assert.fail(`timed out waiting for ${label}`)
}

function sendKey(handle, windowId, key) {
  handle.host.sendEvent(windowId, { type: 'key', key })
}

function sendChar(handle, windowId, char) {
  handle.host.sendEvent(windowId, { type: 'key', char })
}

async function runHeadlessE2e() {
  const handle = render(React.createElement(E2eApp), {
    singleWindow: false,
    headless: true,
    cols: 80,
    rows: 24,
    idPrefix: 'e2e-headless',
  })

  try {
    await waitFor(() => handle.windowIds().length === 2, 'two headless windows')
    assert.deepEqual(handle.host.listWindows().map((window) => window.title), ['E2E Summary', 'E2E Actions'])
    const actionWindowId = handle.windowIds()[1]

    await waitFor(() => findNodeByText(handle.host.snapshot().tree, 'Summary Window'), 'summary window')
    await waitFor(() => findNodeByText(handle.host.snapshot().tree, 'Items total: 1'), 'initial list summary')

    sendChar(handle, actionWindowId, 'x')
    await waitFor(() => findNodeByText(handle.host.snapshot().tree, 'Draft typed: x'), 'controlled TextBox update')

    sendKey(handle, actionWindowId, 'tab')
    sendKey(handle, actionWindowId, 'enter')
    await waitFor(() => findNodeByText(handle.host.snapshot().tree, 'Added item: x'), 'list item add')

    sendKey(handle, actionWindowId, 'tab')
    sendKey(handle, actionWindowId, 'enter')
    await waitFor(() => findNodeByText(handle.host.snapshot().tree, 'Removed item: x'), 'list item remove')

    sendKey(handle, actionWindowId, 'tab')
    sendKey(handle, actionWindowId, 'enter')
    await waitFor(() => findNodeByText(handle.host.snapshot().tree, 'Counter clicked: 1'), 'counter update')
  } finally {
    handle.stop()
  }
}

function runPtyE2e() {
  if (process.platform === 'win32') {
    console.log('React e2e PTY test skipped on win32')
    return
  }

  const app = join(__dirname, 'e2e_app.cjs')
  const runner = String.raw`
import errno
import fcntl
import os
import pty
import select
import struct
import subprocess
import sys
import termios
import time

app = sys.argv[1]
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 80, 0, 0))
env = os.environ.copy()
env.setdefault('TERM', 'xterm-256color')
proc = subprocess.Popen(['node', app], stdin=slave, stdout=slave, stderr=slave, close_fds=True, env=env)
os.close(slave)
output = bytearray()

def read_until(needle, timeout):
    deadline = time.time() + timeout
    while time.time() < deadline:
        readable, _, _ = select.select([master], [], [], 0.05)
        if readable:
            try:
                data = os.read(master, 4096)
            except OSError as error:
                if error.errno == errno.EIO:
                    return needle in output
                raise
            if not data:
                return needle in output
            output.extend(data)
            if needle in output:
                return True
        if proc.poll() is not None:
            return needle in output
    return False

def require_text(needle, label):
    if not read_until(needle, 5):
        proc.kill()
        proc.wait()
        sys.stderr.write(f'timed out waiting for {label}\n')
        sys.stderr.write(bytes(output).decode('utf-8', 'replace'))
        sys.exit(1)

def drain_until_exit(timeout):
    deadline = time.time() + timeout
    while time.time() < deadline:
        readable, _, _ = select.select([master], [], [], 0.05)
        if readable:
            try:
                data = os.read(master, 4096)
            except OSError as error:
                if error.errno == errno.EIO:
                    break
                raise
            if not data:
                break
            output.extend(data)
        if proc.poll() is not None:
            break
    try:
        proc.wait(timeout=0.1)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()

require_text(b'Summary Window', 'summary window')
require_text(b'E2E Actions', 'actions window')

os.write(master, b'x')
require_text(b'Draft typed: x', 'controlled TextBox update')

os.write(master, b'\t')
os.write(master, b'\r')
require_text(b'Added item: x', 'list item add')

os.write(master, b'\t')
os.write(master, b'\r')
require_text(b'Removed item: x', 'list item remove')

os.write(master, b'\t')
os.write(master, b'\r')
require_text(b'Counter clicked: 1', 'counter update')

os.write(master, b'\x11')
drain_until_exit(5)

if proc.returncode != 0:
    sys.stderr.write(f'child exited with {proc.returncode}\n')
    sys.stderr.write(bytes(output).decode('utf-8', 'replace'))
    sys.exit(1)
`

  const result = spawnSync(process.env.PYTHON ?? 'python3', ['-c', runner, app], {
    encoding: 'utf8',
    timeout: 20_000,
  })

  assert.equal(
    result.status,
    0,
    `React e2e PTY test failed\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
  )
}

async function main() {
  await runHeadlessE2e()
  runPtyE2e()
}

main().catch((error) => {
  console.error(error)
  process.exitCode = 1
})
