'use strict'

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
  assert.equal(core.version(), '0.1.0')

  const host = new core.AppHost({ headless: true, cols: 52, rows: 14, tickRate: 0 })
  try {
    const callback = host.allocCallback()
    const windowId = host.addDynamicWindow(
      'Runtime Compat',
      { x: 1, y: 1, width: 36, height: 9 },
      core.VStack({ id: 'compat-root', spacing: 1 }, [
        core.Text('Runtime Compat Ready', { id: 'compat-title' }),
        core.Button({ id: 'compat-button', text: 'Ping', onClick: callback }),
      ]),
    )

    assert.match(callback, /^atto:callback:/)
    assert.match(windowId, /^atto:window:/)
    assert.equal(host.step(), true)
    assert.equal(findNode(host.snapshot().tree, 'compat-title').text, 'Runtime Compat Ready')

    assert.equal(host.applyTreeOps(windowId, {
      op: 'set_prop',
      id: 'compat-title',
      name: 'text',
      value: 'Runtime Compat Updated',
    }), false)
    assert.equal(host.getProperty('compat-title', 'text'), 'Runtime Compat Updated')

    assert.equal(host.sendEvent(windowId, { type: 'key', key: 'enter' }).consumed, true)
    assert.deepEqual(host.drainCallbacks(), [
      { callbackId: callback, targetId: 'compat-button', event: 'click', payload: null },
    ])
  } finally {
    host.dispose()
  }
}

main().catch((error) => {
  console.error(error)
  process.exitCode = 1
})
