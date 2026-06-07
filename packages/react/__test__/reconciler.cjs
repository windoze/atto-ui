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
assert.equal(root.container.rootChildren[0].parent, root.container)
assert.equal(root.container.rootChildren[0].windowId, 'window-1')
assert.equal(root.container.rootChildren[0].children[0].parent, root.container.rootChildren[0])
assert.equal(root.container.rootChildren[0].children[0].windowId, 'window-1')

const stableOps = []
const stableHost = {
  applyTreeOps(windowId, op) {
    stableOps.push({ windowId, op })
    return true
  },
}
const stableRoot = createRoot(stableHost, 'stable-window', { idPrefix: 'stable' })
const stableElement = React.createElement(
  'vstack',
  null,
  React.createElement('label', { text: 'Stable' }),
)
stableRoot.render(stableElement)
const stableRootId = stableRoot.container.rootChildren[0].id
const stableChildId = stableRoot.container.rootChildren[0].children[0].id
stableRoot.render(stableElement)
assert.equal(stableRoot.container.rootChildren[0].id, stableRootId)
assert.equal(stableRoot.container.rootChildren[0].children[0].id, stableChildId)

const firstDefaultRoot = createRoot(host, 'default-1')
const secondDefaultRoot = createRoot(host, 'default-2')
firstDefaultRoot.render(React.createElement('label', { text: 'First' }))
secondDefaultRoot.render(React.createElement('label', { text: 'Second' }))
assert.notEqual(
  firstDefaultRoot.container.rootChildren[0].id,
  secondDefaultRoot.container.rootChildren[0].id,
)
