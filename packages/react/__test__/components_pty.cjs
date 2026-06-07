const assert = require('node:assert/strict')
const { spawnSync } = require('node:child_process')
const { join } = require('node:path')

if (process.platform === 'win32') {
  console.log('components PTY test skipped on win32')
  process.exit(0)
}

const app = join(__dirname, 'components_pty_app.cjs')
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

require_text(b'Name', 'initial controlled TextBox')
os.write(master, b'Z')
require_text(b'Typed: Z', 'controlled TextBox update')

os.write(master, b'\t')
os.write(master, b'\r')
require_text(b'Button: 1', 'Button onClick update')

os.write(master, b'\t')
os.write(master, b'\x1b[B')
require_text(b'List: Beta', 'ListBox onSelect update')

os.write(master, b'\t')
os.write(master, b'\x1b[B')
require_text(b'Table: Row B', 'Table onSelect update')

os.write(master, b'\x11')
drain_until_exit(5)

if proc.returncode != 0:
    sys.stderr.write(f'child exited with {proc.returncode}\n')
    sys.stderr.write(bytes(output).decode('utf-8', 'replace'))
    sys.exit(1)
`

const result = spawnSync(process.env.PYTHON ?? 'python3', ['-c', runner, app], {
  encoding: 'utf8',
  timeout: 18_000,
})

assert.equal(
  result.status,
  0,
  `PTY components test failed\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
)
