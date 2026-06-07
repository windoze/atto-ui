const assert = require('node:assert/strict')
const React = require('react')
const { render } = require('../dist')

function findNode(node, id) {
  if (node.id === id) return node
  for (const child of node.children ?? []) {
    const found = findNode(child, id)
    if (found) return found
  }
  return undefined
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

async function waitFor(predicate) {
  const deadline = Date.now() + 1000
  while (Date.now() < deadline) {
    const result = predicate()
    if (result) return result
    await delay(10)
  }
  assert.fail('timed out waiting for render loop')
}

async function main() {
  const handle = render(
    React.createElement('label', { text: 'Rendered through render()' }),
    { headless: true, cols: 50, rows: 14, idPrefix: 'render' },
  )

  const label = await waitFor(() => findNode(handle.host.snapshot().tree, 'render-1'))
  assert.equal(label.name, 'Label')
  assert.equal(label.text, 'Rendered through render()')

  let promiseRan = false
  Promise.resolve().then(() => {
    promiseRan = true
  })
  await delay(0)
  assert.equal(promiseRan, true)

  handle.stop()
  handle.stop()
  assert.deepEqual(handle.host.listWindows(), [])
}

main().catch((error) => {
  console.error(error)
  process.exitCode = 1
})
