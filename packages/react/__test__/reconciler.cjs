const assert = require('node:assert/strict')
const React = require('react')
const { createRoot } = require('../dist')

function createMockHost() {
  const ops = []
  let nextCallback = 0
  return {
    ops,
    host: {
      applyTreeOps(windowId, op) {
        ops.push({ windowId, op })
        return true
      },
      allocCallback() {
        nextCallback += 1
        return `callback-${nextCallback}`
      },
    },
  }
}

const { host, ops } = createMockHost()

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

const { host: stableHost } = createMockHost()
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

const { host: stateHost, ops: stateOps } = createMockHost()
const stateRoot = createRoot(stateHost, 'state-window', { idPrefix: 'state' })
let setLabelText = null
function StatefulLabel() {
  const [text, setText] = React.useState('Before')
  setLabelText = setText
  return React.createElement('label', { text })
}
stateRoot.render(React.createElement(StatefulLabel))
stateOps.length = 0
setLabelText('After')
assert.deepEqual(stateOps, [
  {
    windowId: 'state-window',
    op: { op: 'set_prop', id: 'state-1', name: 'text', value: 'After' },
  },
])

function LabelList({ items }) {
  return React.createElement(
    'vstack',
    null,
    items.map((item) => React.createElement('label', { key: item, text: item })),
  )
}

const { host: listHost, ops: listOps } = createMockHost()
const listRoot = createRoot(listHost, 'list-window', { idPrefix: 'list' })
listRoot.render(React.createElement(LabelList, { items: ['A', 'B'] }))
const listParent = listRoot.container.rootChildren[0]
const aId = listParent.children[0].id
const bId = listParent.children[1].id
listOps.length = 0

listRoot.render(React.createElement(LabelList, { items: ['A', 'C', 'B'] }))
const cId = listParent.children[1].id
assert.deepEqual(listOps, [
  {
    windowId: 'list-window',
    op: {
      op: 'insert_before',
      parent_id: listParent.id,
      anchor_id: bId,
      child: { type: 'Label', id: cId, props: { text: 'C' } },
    },
  },
])

listOps.length = 0
listRoot.render(React.createElement(LabelList, { items: ['C', 'A', 'B'] }))
assert.deepEqual(listOps, [
  {
    windowId: 'list-window',
    op: {
      op: 'insert_before',
      parent_id: listParent.id,
      anchor_id: bId,
      child: { type: 'Label', id: aId, props: { text: 'A' } },
    },
  },
])

listOps.length = 0
listRoot.render(React.createElement(LabelList, { items: ['C', 'B'] }))
assert.deepEqual(listOps, [
  {
    windowId: 'list-window',
    op: { op: 'remove', id: aId },
  },
])

const { host: eventHost, ops: eventOps } = createMockHost()
const eventRoot = createRoot(eventHost, 'event-window', { idPrefix: 'event' })
eventRoot.render(React.createElement('button', { label: 'Push' }))
const buttonId = eventRoot.container.rootChildren[0].id
eventOps.length = 0

eventRoot.render(React.createElement('button', { label: 'Push', onClick() {} }))
assert.deepEqual(eventOps, [
  {
    windowId: 'event-window',
    op: { op: 'bind_event', id: buttonId, event: 'click', callback: 'callback-1' },
  },
])

eventOps.length = 0
eventRoot.render(React.createElement('button', { label: 'Push', onClick() {} }))
assert.deepEqual(eventOps, [])
assert.equal(eventRoot.container.rootChildren[0].events.click.callbackId, 'callback-1')

eventRoot.render(React.createElement('button', { label: 'Push' }))
assert.deepEqual(eventOps, [
  {
    windowId: 'event-window',
    op: { op: 'clear_event', id: buttonId, event: 'click' },
  },
])
