const assert = require('node:assert/strict')
const { AppHost } = require('..')

// Window lifecycle events are derived by diffing the window list against a
// baseline. JS-initiated mutations patch the baseline immediately, so they must
// never echo back as events (which would otherwise cause an onClose/onMinimize
// feedback loop in the React layer). TUI-initiated changes are covered by the
// interactive demo; headless hosts cannot inject desktop-level titlebar input.

function root(prefix) {
  return {
    type: 'VStack',
    id: `${prefix}-root`,
    children: [{ type: 'Label', id: `${prefix}-label`, props: { text: 'hi' } }],
  }
}

const host = new AppHost({ headless: true, cols: 80, rows: 24, tickRate: 0 })

// A freshly added window does not emit any event.
const w1 = host.addDynamicWindow('One', [1, 1, 20, 6], root('a'))
assert.equal(host.step(), true)
assert.deepEqual(host.drainWindowEvents(), [], 'add must not emit')

// Minimize / restore / maximize via the binding must not echo back.
assert.equal(host.minimizeWindow(w1), true)
assert.equal(host.step(), true)
assert.deepEqual(host.drainWindowEvents(), [], 'minimize must not echo')

assert.equal(host.restoreWindow(w1), true)
assert.equal(host.step(), true)
assert.deepEqual(host.drainWindowEvents(), [], 'restore must not echo')

assert.equal(host.maximizeWindow(w1), true)
assert.equal(host.step(), true)
assert.deepEqual(host.drainWindowEvents(), [], 'maximize must not echo')

// Closing through the binding must not emit a closed event.
const w2 = host.addDynamicWindow('Two', [22, 1, 20, 6], root('b'))
assert.equal(host.step(), true)
host.drainWindowEvents()
assert.equal(host.closeWindow(w2), true)
assert.equal(host.step(), true)
assert.deepEqual(host.drainWindowEvents(), [], 'close must not echo')

host.dispose()
console.log('window lifecycle ok')
