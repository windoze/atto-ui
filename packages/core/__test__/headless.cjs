const assert = require('node:assert/strict')
const core = require('..')

function findNode(node, id) {
  if (node.id === id) return node
  for (const child of node.children ?? []) {
    const found = findNode(child, id)
    if (found) return found
  }
  return undefined
}

async function main() {
  const imported = await import('../index.js')
  assert.equal(imported.version(), core.version())

  const host = new core.AppHost({ headless: true, cols: 48, rows: 14, tickRate: 0 })
  const callback = host.allocCallback()
  assert.match(callback, /^atto:callback:/)

  const windowId = host.addDynamicWindow('Core Smoke', { x: 1, y: 1, width: 32, height: 9 }, {
    type: 'VStack',
    id: 'root',
    children: [
      { type: 'Label', id: 'title', props: { text: 'Before' } },
      { type: 'Button', id: 'ok', props: { label: 'OK' }, events: { click: callback } },
    ],
  })

  assert.match(windowId, /^atto:window:/)
  assert.equal(host.step(), true)
  assert.equal(host.applyTreeOps(windowId, { op: 'set_prop', id: 'title', name: 'text', value: 'After' }), false)
  assert.equal(host.getProperty('title', 'text'), 'After')
  assert.equal(host.applyTreeOps(windowId, [
    { op: 'insert_before', parent_id: 'root', anchor_id: 'ok', child: { type: 'Label', id: 'pre', props: { text: 'Pre' } } },
    { op: 'insert_before', parent_id: 'root', anchor_id: null, child: { type: 'Label', id: 'tail', props: { text: 'Tail' } } },
    { op: 'insert_before', parent_id: 'root', anchor_id: 'tail', child: { type: 'Label', id: 'title' } },
  ]), true)

  const snapshot = host.snapshot()
  assert.equal(snapshot.bounds.width, 48)
  assert.deepEqual(findNode(snapshot.tree, 'root').children.map((child) => child.id), ['pre', 'ok', 'title', 'tail'])
  assert.equal(findNode(snapshot.tree, 'title').text, 'After')

  const eventResult = host.sendEvent(windowId, { type: 'key', key: 'enter' })
  assert.equal(eventResult.consumed, true)
  assert.deepEqual(host.drainCallbacks(), [
    { callbackId: callback, targetId: 'ok', event: 'click', payload: null },
  ])

  assert.equal(host.closeWindow(windowId), true)
  assert.throws(() => host.focusWindow(windowId), /unknown window id handle/)
}

main().catch((error) => {
  console.error(error)
  process.exitCode = 1
})
