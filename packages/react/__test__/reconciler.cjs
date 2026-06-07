const assert = require('node:assert/strict')
const React = require('react')
const {
  B,
  Button,
  Grid,
  ListBox,
  I,
  Link,
  Markdown,
  Menu,
  MenuBar,
  MenuItem,
  S,
  StatusBar,
  Table,
  Text,
  TextBox,
  U,
  VStack,
  Window,
  createDesktopRoot,
  createRoot,
  dispatchHostCallbacks,
} = require('../dist')

function createMockHost() {
  const ops = []
  const releasedCallbacks = []
  const windows = []
  const closedWindows = []
  const titles = []
  const moves = []
  const resizes = []
  const menus = []
  const statusBars = []
  let nextCallback = 0
  let nextWindow = 0
  return {
    ops,
    releasedCallbacks,
    windows,
    closedWindows,
    titles,
    moves,
    resizes,
    menus,
    statusBars,
    host: {
      addDynamicWindow(title, rect, root) {
        nextWindow += 1
        const id = `dynamic-window-${nextWindow}`
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
      moveWindow(windowId, x, y) {
        moves.push({ windowId, x, y })
        return true
      },
      resizeWindow(windowId, width, height) {
        resizes.push({ windowId, width, height })
        return true
      },
      setTitle(windowId, title) {
        titles.push({ windowId, title })
        return true
      },
      setMenuBar(spec) {
        menus.push(spec)
      },
      setStatusBar(left, right) {
        statusBars.push({ left, right })
      },
      allocCallback() {
        nextCallback += 1
        return `callback-${nextCallback}`
      },
      releaseCallback(callbackId) {
        releasedCallbacks.push(callbackId)
        return true
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

const SharedContext = React.createContext('initial')
function SharedLabel({ prefix }) {
  const value = React.useContext(SharedContext)
  return React.createElement('label', { text: `${prefix}:${value}` })
}
function MultiWindowApp({ value, showSecond = true }) {
  return React.createElement(
    SharedContext.Provider,
    { value },
    React.createElement(Window, { title: 'One', rect: [1, 1, 20, 6] },
      React.createElement(SharedLabel, { prefix: 'one' })),
    showSecond && React.createElement(Window, { title: 'Two', rect: [24, 1, 20, 6] },
      React.createElement('vstack', null, React.createElement(SharedLabel, { prefix: 'two' }))),
  )
}

const {
  host: desktopHost,
  ops: desktopOps,
  windows: desktopWindows,
  closedWindows: desktopClosedWindows,
} = createMockHost()
const desktopRoot = createDesktopRoot(desktopHost, { idPrefix: 'desktop' })
desktopRoot.render(React.createElement(MultiWindowApp, { value: 'A' }))
assert.equal(desktopWindows.length, 2)
assert.equal(desktopWindows[0].title, 'One')
assert.deepEqual(desktopWindows[0].root.props, { text: 'one:A' })
assert.equal(desktopWindows[1].root.children[0].props.text, 'two:A')
const firstWindow = desktopRoot.container.rootChildren[0]
const secondWindow = desktopRoot.container.rootChildren[1]
const firstWindowLabel = firstWindow.children[0]
const secondWindowLabel = secondWindow.children[0].children[0]
assert.equal(firstWindow.windowId, desktopWindows[0].id)
assert.equal(secondWindow.children[0].windowId, desktopWindows[1].id)

desktopOps.length = 0
desktopRoot.render(React.createElement(MultiWindowApp, { value: 'B' }))
assert.deepEqual(desktopOps, [
  {
    windowId: desktopWindows[0].id,
    op: { op: 'set_prop', id: firstWindowLabel.id, name: 'text', value: 'one:B' },
  },
  {
    windowId: desktopWindows[1].id,
    op: { op: 'set_prop', id: secondWindowLabel.id, name: 'text', value: 'two:B' },
  },
])

desktopRoot.render(React.createElement(MultiWindowApp, { value: 'B', showSecond: false }))
assert.deepEqual(desktopClosedWindows, [desktopWindows[1].id])

const {
  host: windowPropHost,
  titles: windowPropTitles,
  moves: windowPropMoves,
  resizes: windowPropResizes,
} = createMockHost()
const windowPropRoot = createDesktopRoot(windowPropHost, { idPrefix: 'window-prop' })
windowPropRoot.render(React.createElement(Window, { title: 'Before', rect: [1, 2, 20, 6] },
  React.createElement('label', { text: 'body' })))
const windowPropId = windowPropRoot.container.rootChildren[0].windowId
windowPropRoot.render(React.createElement(Window, { title: 'After', rect: [3, 4, 24, 8] },
  React.createElement('label', { text: 'body' })))
assert.deepEqual(windowPropTitles, [{ windowId: windowPropId, title: 'After' }])
assert.deepEqual(windowPropMoves, [{ windowId: windowPropId, x: 3, y: 4 }])
assert.deepEqual(windowPropResizes, [{ windowId: windowPropId, width: 24, height: 8 }])

const { host: invalidDesktopHost } = createMockHost()
const invalidDesktopRoot = createDesktopRoot(invalidDesktopHost, { idPrefix: 'invalid-desktop' })
const originalConsoleError = console.error
console.error = () => {}
try {
  assert.throws(
    () => invalidDesktopRoot.render(React.createElement('label', { text: 'not a window' })),
    /DesktopContainer direct children/,
  )
} finally {
  console.error = originalConsoleError
}

const {
  host: chromeHost,
  menus: chromeMenus,
  statusBars: chromeStatusBars,
} = createMockHost()
const chromeRoot = createDesktopRoot(chromeHost, { idPrefix: 'chrome' })
let menuClicked = false
chromeRoot.render(React.createElement(
  React.Fragment,
  null,
  React.createElement(MenuBar, null,
    React.createElement(Menu, { title: 'File' },
      React.createElement(MenuItem, { label: 'Open', shortcut: 'Ctrl+O', onClick() { menuClicked = true } }))),
  React.createElement(StatusBar, { left: 'Ready', right: 'Ln 1' }),
  React.createElement(Window, { title: 'Main', rect: [1, 1, 20, 6] },
    React.createElement('label', { text: 'body' })),
))
assert.deepEqual(chromeMenus[0].menus[0], {
  id: 'chrome-2',
  title: 'File',
  items: [{ id: 'chrome-1', label: 'Open', shortcut: 'Ctrl+O', enabled: true, callback: 'callback-1', items: [] }],
})
assert.deepEqual(chromeStatusBars, [{ left: 'Ready', right: 'Ln 1' }])
assert.equal(dispatchHostCallbacks(chromeRoot.container, [
  { callbackId: 'callback-1', targetId: 'chrome-1', event: 'click', payload: null },
]), 1)
assert.equal(menuClicked, true)

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

const { host: propHost, ops: propOps } = createMockHost()
const propRoot = createRoot(propHost, 'prop-window', { idPrefix: 'prop' })
propRoot.render(React.createElement('label', { text: 'Stable', enabled: true }))
const propId = propRoot.container.rootChildren[0].id
propOps.length = 0

propRoot.render(React.createElement('label', { text: 'Stable' }))
assert.deepEqual(propOps, [
  {
    windowId: 'prop-window',
    op: { op: 'clear_prop', id: propId, name: 'enabled' },
  },
])
assert.deepEqual(propRoot.container.rootChildren[0].props, { text: 'Stable' })

propOps.length = 0
propRoot.render(React.createElement('label', { text: 'Changed' }))
assert.deepEqual(propOps, [
  {
    windowId: 'prop-window',
    op: { op: 'set_prop', id: propId, name: 'text', value: 'Changed' },
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

const { host: appendMoveHost, ops: appendMoveOps } = createMockHost()
const appendMoveRoot = createRoot(appendMoveHost, 'append-move-window', { idPrefix: 'append-move' })
appendMoveRoot.render(React.createElement(LabelList, { items: ['A', 'B', 'C'] }))
const appendMoveParent = appendMoveRoot.container.rootChildren[0]
const appendMoveAId = appendMoveParent.children[0].id
appendMoveOps.length = 0

appendMoveRoot.render(React.createElement(LabelList, { items: ['B', 'C', 'A'] }))
assert.deepEqual(appendMoveOps, [
  {
    windowId: 'append-move-window',
    op: {
      op: 'insert_before',
      parent_id: appendMoveParent.id,
      anchor_id: null,
      child: { type: 'Label', id: appendMoveAId, props: { text: 'A' } },
    },
  },
])

const { host: eventHost, ops: eventOps, releasedCallbacks: eventReleasedCallbacks } = createMockHost()
const eventRoot = createRoot(eventHost, 'event-window', { idPrefix: 'event' })
eventRoot.render(React.createElement('button', { label: 'Push' }))
const buttonId = eventRoot.container.rootChildren[0].id
eventOps.length = 0

let clicked = 'initial'
eventRoot.render(React.createElement('button', { label: 'Push', onClick() { clicked = 'first' } }))
assert.deepEqual(eventOps, [
  {
    windowId: 'event-window',
    op: { op: 'bind_event', id: buttonId, event: 'click', callback: 'callback-1' },
  },
])
assert.equal(dispatchHostCallbacks(eventRoot.container, [
  { callbackId: 'callback-1', targetId: buttonId, event: 'click', payload: null },
]), 1)
assert.equal(clicked, 'first')

eventOps.length = 0
eventRoot.render(React.createElement('button', { label: 'Push', onClick() { clicked = 'second' } }))
assert.deepEqual(eventOps, [])
assert.equal(eventRoot.container.rootChildren[0].events.click.callbackId, 'callback-1')
assert.equal(dispatchHostCallbacks(eventRoot.container, [
  { callbackId: 'callback-1', targetId: buttonId, event: 'click', payload: null },
]), 1)
assert.equal(clicked, 'second')

eventRoot.render(React.createElement('button', { label: 'Push' }))
assert.deepEqual(eventOps, [
  {
    windowId: 'event-window',
    op: { op: 'clear_event', id: buttonId, event: 'click' },
  },
])
assert.deepEqual(eventReleasedCallbacks, ['callback-1'])
assert.equal(dispatchHostCallbacks(eventRoot.container, [
  { callbackId: 'callback-1', targetId: buttonId, event: 'click', payload: null },
]), 0)

function MaybeButton({ show, onClick }) {
  return React.createElement(
    'vstack',
    null,
    show ? React.createElement('button', { label: 'Remove me', onClick }) : null,
  )
}

const {
  host: unmountHost,
  ops: unmountOps,
  releasedCallbacks: unmountReleasedCallbacks,
} = createMockHost()
const unmountRoot = createRoot(unmountHost, 'unmount-window', { idPrefix: 'unmount' })
unmountRoot.render(React.createElement(MaybeButton, { show: true, onClick() {} }))
const unmountParent = unmountRoot.container.rootChildren[0]
const unmountButton = unmountParent.children[0]
unmountOps.length = 0

unmountRoot.render(React.createElement(MaybeButton, { show: false, onClick() {} }))
assert.deepEqual(unmountOps, [
  {
    windowId: 'unmount-window',
    op: [
      { op: 'clear_event', id: unmountButton.id, event: 'click' },
      { op: 'remove', id: unmountButton.id },
    ],
  },
])
assert.deepEqual(unmountReleasedCallbacks, ['callback-1'])
assert.equal(dispatchHostCallbacks(unmountRoot.container, [
  { callbackId: 'callback-1', targetId: unmountButton.id, event: 'click', payload: null },
]), 0)

const { host: rawTextHost, ops: rawTextOps } = createMockHost()
const rawTextRoot = createRoot(rawTextHost, 'raw-text-window', { idPrefix: 'raw-text' })
function RawRichText({ name }) {
  return React.createElement('richText', null, 'hi ', name)
}
rawTextRoot.render(React.createElement(RawRichText, { name: 'Ada' }))
const rawTextTree = rawTextOps[0].op.tree
assert.equal(rawTextTree.type, 'RichText')
assert.deepEqual(rawTextTree.children.map((child) => child.props), [
  { text: 'hi ' },
  { text: 'Ada' },
])
const rawNameSpanId = rawTextTree.children[1].id
rawTextOps.length = 0
rawTextRoot.render(React.createElement(RawRichText, { name: 'Grace' }))
assert.deepEqual(rawTextOps, [
  {
    windowId: 'raw-text-window',
    op: { op: 'set_prop', id: rawNameSpanId, name: 'text', value: 'Grace' },
  },
])

const { host: textHost, ops: textOps } = createMockHost()
const textRoot = createRoot(textHost, 'text-window', { idPrefix: 'text' })
let clickedHref = null
textRoot.render(React.createElement(
  Text,
  null,
  'hi ',
  'Ada',
  React.createElement(B, null, ' bold'),
  React.createElement(I, null, ' italic'),
  React.createElement(U, null, ' under'),
  React.createElement(S, null, ' strike'),
  React.createElement(Link, {
    href: 'https://example.com',
    onClick(event) {
      clickedHref = event.payload
    },
  }, ' link'),
))
const textTree = textOps[0].op.tree
assert.equal(textTree.type, 'RichText')
assert.deepEqual(textTree.children.map((child) => child.props), [
  { text: 'hi ' },
  { text: 'Ada' },
  { text: ' bold', bold: true },
  { text: ' italic', italic: true },
  { text: ' under', underline: true },
  { text: ' strike', strike: true },
  { text: ' link', href: 'https://example.com' },
])
assert.deepEqual(textTree.events, { link: 'callback-1' })
assert.equal(dispatchHostCallbacks(textRoot.container, [
  {
    callbackId: 'callback-1',
    targetId: textTree.id,
    event: 'link',
    payload: 'https://example.com',
  },
]), 1)
assert.equal(clickedHref, 'https://example.com')

const { host: markdownHost, ops: markdownOps } = createMockHost()
const markdownRoot = createRoot(markdownHost, 'markdown-window', { idPrefix: 'markdown' })
markdownRoot.render(React.createElement(Markdown, null, '# Title\n\n- item'))
assert.deepEqual(markdownOps[0].op.tree, {
  type: 'MarkdownViewer',
  id: markdownOps[0].op.tree.id,
  props: { markdown: '# Title\n\n- item' },
})

const { host: componentHost, ops: componentOps } = createMockHost()
const componentRoot = createRoot(componentHost, 'component-window', { idPrefix: 'components' })
let nextTextValue = null
let nextListSelection = null
let nextTableSelection = null
componentRoot.render(React.createElement(
  VStack,
  { spacing: 1 },
  React.createElement(Button, { onClick() {} }, 'Save'),
  React.createElement(TextBox, {
    title: 'Name',
    value: 'Ada',
    onChange(value) {
      nextTextValue = value
    },
  }),
  React.createElement(ListBox, {
    title: 'People',
    items: ['Ada', 'Grace'],
    onSelect(selection) {
      nextListSelection = selection
    },
  }),
  React.createElement(Table, {
    title: 'Rows',
    headers: ['name'],
    rows: [['Ada'], ['Grace']],
    onChange(selection) {
      nextTableSelection = selection
    },
  }),
  React.createElement(Grid, { columns: 2, rowGap: 1, columnGap: 1 },
    React.createElement('label', { text: 'Grid A' }),
    React.createElement('label', { text: 'Grid B' })),
))
const componentTree = componentOps[0].op.tree
assert.equal(componentTree.type, 'VStack')
assert.deepEqual(componentTree.props, { spacing: 1 })
assert.deepEqual(componentTree.children.map((child) => child.type), [
  'Button',
  'TextBox',
  'ListBox',
  'TableView',
  'Grid',
])
const wrappedButton = componentTree.children[0]
const wrappedTextBox = componentTree.children[1]
const wrappedList = componentTree.children[2]
const wrappedTable = componentTree.children[3]
const wrappedGrid = componentTree.children[4]
assert.deepEqual(wrappedButton.props, { label: 'Save' })
assert.deepEqual(wrappedTextBox.props, { title: 'Name', text: 'Ada' })
assert.deepEqual(wrappedList.props, { title: 'People', items: ['Ada', 'Grace'], selection: 0 })
assert.deepEqual(wrappedTable.props, { title: 'Rows', headers: ['name'], rows: [['Ada'], ['Grace']], selection: 0 })
assert.deepEqual(wrappedGrid.props, { columns: 2, row_gap: 1, column_gap: 1 })

assert.equal(dispatchHostCallbacks(componentRoot.container, [
  { callbackId: wrappedTextBox.events.change, targetId: wrappedTextBox.id, event: 'change', payload: 'Grace' },
]), 1)
assert.equal(nextTextValue, 'Grace')
assert.equal(dispatchHostCallbacks(componentRoot.container, [
  { callbackId: wrappedList.events.change, targetId: wrappedList.id, event: 'change', payload: 1 },
]), 1)
assert.equal(nextListSelection, 1)
assert.equal(dispatchHostCallbacks(componentRoot.container, [
  { callbackId: wrappedTable.events.change, targetId: wrappedTable.id, event: 'change', payload: 1 },
]), 1)
assert.equal(nextTableSelection, 1)
