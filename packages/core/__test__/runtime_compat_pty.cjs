'use strict'

const assert = require('node:assert/strict')
const { spawnSync } = require('node:child_process')
const { join } = require('node:path')

if (process.platform === 'win32') {
  console.log('runtime PTY compatibility test skipped on win32')
  process.exit(0)
}

const runtime = process.argv[2] ?? 'node'
const app = join(__dirname, 'runtime_compat_pty_app.cjs')
const command = runtimeCommand(runtime, app)

function runtimeCommand(name, appPath) {
  switch (name) {
    case 'node':
      return [process.env.NODE_BIN ?? process.execPath, appPath]
    case 'bun':
      return [process.env.BUN_BIN ?? 'bun', appPath]
    case 'deno':
      return [
        process.env.DENO_BIN ?? 'deno',
        'run',
        '--allow-read',
        '--allow-env',
        '--allow-run',
        '--allow-ffi',
        '--allow-sys',
        appPath,
      ]
    default:
      throw new Error(`unknown runtime: ${name}`)
  }
}

const runner = String.raw`
import errno
import fcntl
import json
import os
import pty
import select
import struct
import subprocess
import sys
import termios
import time

command = json.loads(sys.argv[1])
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 80, 0, 0))
raw_mode_mask = termios.ICANON | termios.ECHO
original_lflag = termios.tcgetattr(master)[3] & raw_mode_mask
env = os.environ.copy()
env.setdefault('TERM', 'xterm-256color')
proc = subprocess.Popen(command, stdin=slave, stdout=slave, stderr=slave, close_fds=True, env=env)
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

if not read_until(b'Runtime PTY Ready', 8):
    proc.kill()
    proc.wait()
    sys.stderr.write('timed out waiting for Runtime PTY Ready\n')
    sys.stderr.write(bytes(output).decode('utf-8', 'replace'))
    sys.exit(1)

os.write(master, b'\x11')
drain_until_exit(8)
raw = bytes(output)

if proc.returncode != 0:
    sys.stderr.write(f'child exited with {proc.returncode}\n')
    sys.stderr.write(raw.decode('utf-8', 'replace'))
    sys.exit(1)
if b'\x1b[?1049l' not in raw:
    sys.stderr.write('missing alternate-screen restore sequence\n')
    sys.exit(1)
if b'\x1b[?25h' not in raw:
    sys.stderr.write('missing cursor-show restore sequence\n')
    sys.exit(1)
if b'\x1b[?1000l' not in raw:
    sys.stderr.write('missing mouse-capture restore sequence\n')
    sys.exit(1)
restored_lflag = termios.tcgetattr(master)[3] & raw_mode_mask
if restored_lflag != original_lflag:
    sys.stderr.write('raw mode flags were not restored\n')
    sys.exit(1)
`

const result = spawnSync(process.env.PYTHON ?? 'python3', ['-c', runner, JSON.stringify(command)], {
  encoding: 'utf8',
  timeout: 20_000,
})

assert.equal(
  result.status,
  0,
  `${runtime} PTY compatibility test failed\ncommand: ${command.join(' ')}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
)
