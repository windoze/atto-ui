const assert = require('node:assert/strict')
const React = require('react')
const {
  Window,
  createDesktopRoot,
  createRoot,
  dispatchHostCallbacks,
} = require('../dist')

// This file intentionally uses a mock host so HostConfig -> TreeOp lowering stays pure JS.
function createMockHost() {
  const ops = []
  const releasedCallbacks = []
  const windows = []
  const closedWindows = []
  let nextCallback = 0
  let nextWindow = 0

  return {
    ops,
    releasedCallbacks,
    windows,
    closedWindows,
    host: {
      addDynamicWindow(title, rect, root) {
        nextWindow += 1
        const id = `matrix-window-${nextWindow}`
        windows.push({ id, title, rect, root })
        return id
      },
      applyTreeOps(windowId, op) {
        ops.push({ windowId, op })
        return true
      },
      closeWindow(windowId) {
        closedWindows.push(windowId)
        return true
      },
      moveWindow() {
        return true
      },
      resizeWindow() {
        return true
      },
      setTitle() {
        return true
      },
      setMenuBar() {},
      setStatusBar() {},
      allocCallback() {
        nextCallback += 1
        return `matrix-callback-${nextCallback}`
      },
      releaseCallback(callbackId) {
        releasedCallbacks.push(callbackId)
        return true
      },
    },
  }
}

function h(type, props, ...children) {
  return React.createElement(type, props, ...children)
}

function labels(items) {
  return h('vstack', null, ...items.map((item) => h('label', { key: item, text: item })))
}

function keyedLabels(items) {
  return h('vstack', null, ...items.map((item) => h('label', { key: item.key, text: item.text })))
}

{
  const { host, ops } = createMockHost()
  const root = createRoot(host, 'mount-window', { idPrefix: 'mount' })

  root.render(h('vstack', { spacing: 2 }, h('label', { text: 'A' }), h('button', { label: 'B' })))

  assert.deepEqual(ops, [{
    windowId: 'mount-window',
    op: {
      op: 'set_tree',
      tree: {
        type: 'VStack',
        id: 'mount-3',
        props: { spacing: 2 },
        children: [
          { type: 'Label', id: 'mount-1', props: { text: 'A' } },
          { type: 'Button', id: 'mount-2', props: { label: 'B' } },
        ],
      },
    },
  }])
}

{
  const { host, ops, releasedCallbacks } = createMockHost()
  const root = createRoot(host, 'prop-window', { idPrefix: 'prop-matrix' })
  function noop() {}
  let focused = false
  function focus() {
    focused = true
  }

  root.render(h('button', { label: 'Old', tooltip: 'Tip', onClick: noop }))
  const button = root.container.rootChildren[0]
  ops.length = 0

  root.render(h('button', { label: 'New', disabled: true, onFocus: focus }))

  assert.deepEqual(ops, [{
    windowId: 'prop-window',
    op: [
      { op: 'set_prop', id: button.id, name: 'label', value: 'New' },
      { op: 'set_prop', id: button.id, name: 'disabled', value: true },
      { op: 'clear_prop', id: button.id, name: 'tooltip' },
      { op: 'clear_event', id: button.id, event: 'click' },
      { op: 'bind_event', id: button.id, event: 'focus', callback: 'matrix-callback-2' },
    ],
  }])
  assert.deepEqual(releasedCallbacks, ['matrix-callback-1'])
  assert.equal(dispatchHostCallbacks(root.container, [
    { callbackId: 'matrix-callback-1', targetId: button.id, event: 'click', payload: null },
  ]), 0)
  assert.equal(dispatchHostCallbacks(root.container, [
    { callbackId: 'matrix-callback-2', targetId: button.id, event: 'focus', payload: null },
  ]), 1)
  assert.equal(focused, true)
}

{
  const { host, ops } = createMockHost()
  const root = createRoot(host, 'text-window', { idPrefix: 'text-matrix' })

  root.render(h('richText', null, 'before'))
  const textSpan = root.container.rootChildren[0].children[0]
  ops.length = 0

  root.render(h('richText', null, 'after'))

  assert.deepEqual(ops, [{
    windowId: 'text-window',
    op: { op: 'set_prop', id: textSpan.id, name: 'text', value: 'after' },
  }])
}

{
  const { host, ops } = createMockHost()
  const root = createRoot(host, 'insert-window', { idPrefix: 'insert-matrix' })

  root.render(labels(['A']))
  const parent = root.container.rootChildren[0]
  const aId = parent.children[0].id
  ops.length = 0

  root.render(labels(['A', 'B']))
  const bId = parent.children[1].id
  assert.deepEqual(ops, [{
    windowId: 'insert-window',
    op: {
      op: 'insert_before',
      parent_id: parent.id,
      anchor_id: null,
      child: { type: 'Label', id: bId, props: { text: 'B' } },
    },
  }])

  ops.length = 0
  root.render(labels(['C', 'A', 'B']))
  const cId = parent.children[0].id
  assert.deepEqual(ops, [{
    windowId: 'insert-window',
    op: {
      op: 'insert_before',
      parent_id: parent.id,
      anchor_id: aId,
      child: { type: 'Label', id: cId, props: { text: 'C' } },
    },
  }])
}

{
  const { host, ops } = createMockHost()
  const root = createRoot(host, 'move-window', { idPrefix: 'move-matrix' })

  root.render(labels(['A', 'B', 'C']))
  const parent = root.container.rootChildren[0]
  const aId = parent.children[0].id
  ops.length = 0

  root.render(labels(['B', 'C', 'A']))

  assert.deepEqual(ops, [{
    windowId: 'move-window',
    op: {
      op: 'insert_before',
      parent_id: parent.id,
      anchor_id: null,
      child: { type: 'Label', id: aId, props: { text: 'A' } },
    },
  }])
}

{
  const { host, ops, releasedCallbacks } = createMockHost()
  const root = createRoot(host, 'remove-window', { idPrefix: 'remove-matrix' })
  function maybeButton(show) {
    return h('vstack', null, show ? h('button', { label: 'Remove', onClick() {} }) : null)
  }

  root.render(maybeButton(true))
  const parent = root.container.rootChildren[0]
  const button = parent.children[0]
  ops.length = 0

  root.render(maybeButton(false))

  assert.deepEqual(ops, [{
    windowId: 'remove-window',
    op: [
      { op: 'clear_event', id: button.id, event: 'click' },
      { op: 'remove', id: button.id },
    ],
  }])
  assert.deepEqual(releasedCallbacks, ['matrix-callback-1'])
  assert.equal(dispatchHostCallbacks(root.container, [
    { callbackId: 'matrix-callback-1', targetId: button.id, event: 'click', payload: null },
  ]), 0)
}

{
  const { host, ops } = createMockHost()
  const root = createRoot(host, 'clear-window', { idPrefix: 'clear-matrix' })

  root.render(h('label', { text: 'temporary' }))
  ops.length = 0

  root.render(null)

  assert.deepEqual(ops, [{
    windowId: 'clear-window',
    op: { op: 'set_tree', tree: { type: 'Spacer', id: 'clear-matrix-empty-root' } },
  }])
  assert.equal(root.container.rootChildren.length, 0)
}

{
  const { host, ops, windows, closedWindows } = createMockHost()
  const root = createDesktopRoot(host, { idPrefix: 'desktop-matrix' })
  function App({ first, second, showSecond = true }) {
    return React.createElement(
      React.Fragment,
      null,
      React.createElement(Window, { title: 'One', rect: [1, 1, 20, 5] }, keyedLabels(first)),
      showSecond && React.createElement(Window, { title: 'Two', rect: [22, 1, 20, 5] }, keyedLabels(second)),
    )
  }

  root.render(React.createElement(App, {
    first: [{ key: 'a', text: 'A' }, { key: 'b', text: 'B' }],
    second: [{ key: 'x', text: 'X' }, { key: 'y', text: 'Y' }],
  }))
  assert.equal(ops.length, 0)
  assert.equal(windows.length, 2)
  const firstWindowId = windows[0].id
  const secondWindowId = windows[1].id
  const firstParent = root.container.rootChildren[0].children[0]
  const secondParent = root.container.rootChildren[1].children[0]
  const firstAId = firstParent.children[0].id
  const firstBId = firstParent.children[1].id
  const secondXId = secondParent.children[0].id
  ops.length = 0

  root.render(React.createElement(App, {
    first: [{ key: 'a', text: 'A1' }, { key: 'b', text: 'B1' }],
    second: [{ key: 'x', text: 'X1' }, { key: 'y', text: 'Y' }],
  }))

  assert.deepEqual(ops, [
    {
      windowId: firstWindowId,
      op: [
        { op: 'set_prop', id: firstAId, name: 'text', value: 'A1' },
        { op: 'set_prop', id: firstBId, name: 'text', value: 'B1' },
      ],
    },
    {
      windowId: secondWindowId,
      op: { op: 'set_prop', id: secondXId, name: 'text', value: 'X1' },
    },
  ])

  ops.length = 0
  root.render(React.createElement(App, {
    first: [{ key: 'a', text: 'A1' }, { key: 'b', text: 'B1' }],
    second: [{ key: 'x', text: 'X1' }, { key: 'y', text: 'Y' }],
    showSecond: false,
  }))
  assert.deepEqual(ops, [])
  assert.deepEqual(closedWindows, [secondWindowId])
}
