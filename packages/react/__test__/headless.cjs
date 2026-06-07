const assert = require('node:assert/strict')
const React = require('react')
const core = require('../../core')
const { createRoot } = require('../dist')

function findNode(node, id) {
  if (node.id === id) return node
  for (const child of node.children ?? []) {
    const found = findNode(child, id)
    if (found) return found
  }
  return undefined
}

const host = new core.AppHost({ headless: true, cols: 50, rows: 14, tickRate: 0 })
const windowId = host.addDynamicWindow('React Static', { x: 1, y: 1, width: 34, height: 9 }, {
  type: 'Spacer',
  id: 'react-placeholder',
})

const root = createRoot(host, windowId, { idPrefix: 'headless' })
root.render(React.createElement(
  'vstack',
  null,
  React.createElement('label', { text: 'Rendered by React' }),
))

assert.equal(host.step(), true)
const snapshot = host.snapshot()
const label = findNode(snapshot.tree, 'headless-1')
assert.ok(label)
assert.equal(label.name, 'Label')
assert.equal(label.text, 'Rendered by React')
