const assert = require('node:assert/strict')
const { spawnSync } = require('node:child_process')
const { join } = require('node:path')

if (process.platform === 'win32') {
  console.log('render PTY test skipped on win32')
  process.exit(0)
}

const app = join(__dirname, 'render_pty_app.cjs')
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

def read_ready(timeout):
    deadline = time.time() + timeout
    while time.time() < deadline:
        readable, _, _ = select.select([master], [], [], 0.05)
        if readable:
            try:
                data = os.read(master, 4096)
            except OSError as error:
                if error.errno == errno.EIO:
                    return False
                raise
            if not data:
                return False
            output.extend(data)
            if b'React PTY Ready' in output:
                return True
        if proc.poll() is not None:
            return b'React PTY Ready' in output
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

if not read_ready(5):
    proc.kill()
    proc.wait()
    sys.stderr.write('timed out waiting for React PTY Ready\n')
    sys.stderr.write(bytes(output).decode('utf-8', 'replace'))
    sys.exit(1)

os.write(master, b'\x11')
drain_until_exit(5)
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
`

const result = spawnSync(process.env.PYTHON ?? 'python3', ['-c', runner, app], {
  encoding: 'utf8',
  timeout: 12_000,
})

assert.equal(
  result.status,
  0,
  `PTY render test failed\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
)
