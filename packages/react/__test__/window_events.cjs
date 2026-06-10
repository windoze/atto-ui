const assert = require('node:assert/strict')
const React = require('react')
const { render, dispatchWindowEvents, Window, Text } = require('../dist')

// Verifies that window lifecycle events are routed to the right <Window>'s
// callbacks. The binding's diff (TUI-origin detection) is covered separately in
// crates/atto-ui-node/__test__/window_lifecycle.cjs; here we exercise the React
// routing in isolation by dispatching synthetic events.

let minimized = 0
let restored = 0
let maximized = 0
let closed = 0
let lastEvent = null

const element = React.createElement(
  Window,
  {
    title: 'W',
    rect: { x: 0, y: 0, width: 40, height: 10 },
    onMinimize: (event) => {
      minimized += 1
      lastEvent = event
    },
    onRestore: () => {
      restored += 1
    },
    onMaximize: () => {
      maximized += 1
    },
    onClose: () => {
      closed += 1
    },
  },
  React.createElement(Text, null, 'hi'),
)

const handle = render(element, {
  headless: true,
  singleWindow: false,
  cols: 80,
  rows: 24,
  idPrefix: 'wl',
})

try {
  // LegacyRoot commits synchronously, so the window is mounted on return.
  const windowId = handle.windowIds()[0]
  assert.ok(windowId, 'window mounted')

  dispatchWindowEvents(handle.root.container, [{ windowId, type: 'minimized', state: 'Minimized' }])
  dispatchWindowEvents(handle.root.container, [{ windowId, type: 'maximized', state: 'Maximized' }])
  dispatchWindowEvents(handle.root.container, [{ windowId, type: 'restored', state: 'Normal' }])
  dispatchWindowEvents(handle.root.container, [{ windowId, type: 'closed', state: null }])

  assert.equal(minimized, 1, 'onMinimize called once')
  assert.equal(maximized, 1, 'onMaximize called once')
  assert.equal(restored, 1, 'onRestore called once')
  assert.equal(closed, 1, 'onClose called once')
  assert.equal(lastEvent && lastEvent.windowId, windowId, 'event carries windowId')
  assert.equal(lastEvent && lastEvent.type, 'minimized', 'event carries type')

  // Events for an unknown window are ignored.
  dispatchWindowEvents(handle.root.container, [{ windowId: 'atto:window:does-not-exist', type: 'closed', state: null }])
  assert.equal(closed, 1, 'unknown window id is ignored')
} finally {
  handle.stop()
}

console.log('react window events ok')
