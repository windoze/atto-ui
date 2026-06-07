const assert = require('node:assert/strict')
const { AppHost } = require('..')

function findNode(node, id) {
  if (node.id === id) return node
  for (const child of node.children ?? []) {
    const found = findNode(child, id)
    if (found) return found
  }
  return undefined
}

const host = new AppHost({ headless: true, cols: 48, rows: 14, tickRate: 0 })
const callback = host.allocCallback()
assert.match(callback, /^atto:callback:/)

const windowId = host.addDynamicWindow(
  'Smoke',
  [1, 1, 32, 9],
  {
    type: 'VStack',
    id: 'root',
    children: [
      { type: 'Label', id: 'title', props: { text: 'Before' } },
      {
        type: 'Button',
        id: 'ok',
        props: { label: 'OK' },
        events: { click: callback },
      },
    ],
  },
)

assert.match(windowId, /^atto:window:/)
assert.equal(host.step(), true)
assert.equal(
  host.applyTreeOps(windowId, [
    { op: 'set_prop', id: 'title', name: 'text', value: 'After' },
  ]),
  false,
)
assert.equal(host.getProperty('title', 'text'), 'After')

const snapshot = host.snapshot()
assert.equal(snapshot.bounds.width, 48)
assert.equal(findNode(snapshot.tree, 'title').text, 'After')

const eventResult = host.sendEvent(windowId, { type: 'key', key: 'enter' })
assert.equal(eventResult.consumed, true)

const callbacks = host.drainCallbacks()
assert.equal(callbacks.length, 1)
assert.deepEqual(callbacks[0], {
  callbackId: callback,
  targetId: 'ok',
  event: 'click',
  payload: null,
})

const windows = host.listWindows()
assert.equal(windows.length, 1)
assert.equal(windows[0].id, windowId)
assert.equal(windows[0].title, 'Smoke')

assert.equal(host.setTitle(windowId, 'Renamed'), true)
assert.equal(host.listWindows()[0].title, 'Renamed')
assert.equal(host.closeWindow(windowId), true)
assert.throws(() => host.focusWindow(windowId), /unknown window id handle/)
