const assert = require('node:assert/strict')
const React = require('react')
const { createRoot } = require('../dist')

const ops = []
const host = {
  applyTreeOps(windowId, op) {
    ops.push({ windowId, op })
    return true
  },
}

const root = createRoot(host, 'window-1', { idPrefix: 'node' })
root.render(React.createElement(
  'vstack',
  { spacing: 1 },
  React.createElement('label', { text: 'Hello from React' }),
))

assert.deepEqual(ops, [
  {
    windowId: 'window-1',
    op: {
      op: 'set_tree',
      tree: {
        type: 'VStack',
        id: 'node-2',
        props: { spacing: 1 },
        children: [
          {
            type: 'Label',
            id: 'node-1',
            props: { text: 'Hello from React' },
          },
        ],
      },
    },
  },
])

assert.deepEqual(root.container.lastTree.children.map((child) => child.id), ['node-1'])
