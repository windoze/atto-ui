const assert = require('node:assert/strict')
const React = require('react')
const core = require('../../core')
const { Markdown, createRoot, dispatchHostCallbacks } = require('../dist')

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

const eventHost = new core.AppHost({ headless: true, cols: 50, rows: 14, tickRate: 0 })
const eventWindowId = eventHost.addDynamicWindow('React Events', { x: 1, y: 1, width: 34, height: 9 }, {
  type: 'Spacer',
  id: 'react-event-placeholder',
})

function CounterButton() {
  const [count, setCount] = React.useState(0)
  return React.createElement('button', {
    label: `Count: ${count}`,
    onClick() {
      setCount((current) => current + 1)
    },
  })
}

const eventRoot = createRoot(eventHost, eventWindowId, { idPrefix: 'event-headless' })
eventRoot.render(React.createElement(CounterButton))
assert.equal(eventHost.step(), true)
assert.equal(findNode(eventHost.snapshot().tree, 'event-headless-1').text, 'Count: 0')

eventHost.sendEvent(eventWindowId, { type: 'key', key: 'enter' })
assert.equal(dispatchHostCallbacks(eventRoot.container, eventHost.drainCallbacks()), 1)
assert.equal(findNode(eventHost.snapshot().tree, 'event-headless-1').text, 'Count: 1')

const markdownHost = new core.AppHost({ headless: true, cols: 60, rows: 16, tickRate: 0 })
const markdownWindowId = markdownHost.addDynamicWindow('React Markdown', { x: 1, y: 1, width: 40, height: 10 }, {
  type: 'Spacer',
  id: 'react-markdown-placeholder',
})
const markdownRoot = createRoot(markdownHost, markdownWindowId, { idPrefix: 'markdown-headless' })
markdownRoot.render(React.createElement(Markdown, null, '# Title\n\n- item'))
assert.equal(markdownHost.step(), true)
assert.equal(findNode(markdownHost.snapshot().tree, 'markdown-headless-1').name, 'MarkdownViewer')
