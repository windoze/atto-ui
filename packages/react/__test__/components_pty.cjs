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
import unicodedata

app = sys.argv[1]
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 80, 0, 0))
env = os.environ.copy()
env.setdefault('TERM', 'xterm-256color')
proc = subprocess.Popen(['node', app], stdin=slave, stdout=slave, stderr=slave, close_fds=True, env=env)
os.close(slave)
output = bytearray()

# Rebuild the visible terminal grid so text checks match what is on screen, not
# the raw byte order. Ratatui's diff can split a label across cursor moves / SGR
# resets (e.g. "B" + color + "utton: 1") when only some cells change between
# frames, which would defeat a naive substring search over the raw stream.
def char_width(ch):
    return 2 if unicodedata.east_asian_width(ch) in ('W', 'F') else 1

def render_screen(data):
    text = data.decode('utf-8', 'replace')
    cols = 220
    rows = [[' '] * cols for _ in range(28)]
    r = c = 0
    i = 0
    n = len(text)
    while i < n:
        ch = text[i]
        if ch == '\x1b':
            if i + 1 < n and text[i + 1] == '[':
                j = i + 2
                while j < n and not ('\x40' <= text[j] <= '\x7e'):
                    j += 1
                if j >= n:
                    break
                final = text[j]
                params = text[i + 2:j]
                if final in ('H', 'f'):
                    parts = params.split(';')
                    def to_int(p, default):
                        try:
                            return int(p) if p else default
                        except ValueError:
                            return default
                    r = max(0, to_int(parts[0] if parts else '', 1) - 1)
                    c = max(0, to_int(parts[1] if len(parts) > 1 else '', 1) - 1)
                i = j + 1
                continue
            if i + 1 < n and text[i + 1] == ']':
                j = i + 2
                while j < n and text[j] != '\x07' and not (
                    text[j] == '\x1b' and j + 1 < n and text[j + 1] == '\\'
                ):
                    j += 1
                i = j + 1
                continue
            i += 2
            continue
        if ch == '\n':
            r += 1
            i += 1
            continue
        if ch == '\r':
            c = 0
            i += 1
            continue
        if ch >= ' ':
            if 0 <= r < len(rows) and 0 <= c < cols:
                rows[r][c] = ch
            c += char_width(ch)
            i += 1
            continue
        i += 1
    return '\n'.join(''.join(row).rstrip() for row in rows)

def screen_has(needle):
    return needle.decode('utf-8') in render_screen(output)

def read_until(needle, timeout):
    deadline = time.time() + timeout
    while time.time() < deadline:
        readable, _, _ = select.select([master], [], [], 0.05)
        if readable:
            try:
                data = os.read(master, 4096)
            except OSError as error:
                if error.errno == errno.EIO:
                    return screen_has(needle)
                raise
            if not data:
                return screen_has(needle)
            output.extend(data)
            if screen_has(needle):
                return True
        if proc.poll() is not None:
            return screen_has(needle)
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
