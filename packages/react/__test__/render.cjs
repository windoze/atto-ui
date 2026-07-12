const assert = require('node:assert/strict')
const React = require('react')
const { Window, render, useChatMessages } = require('../dist')

function findNode(node, id) {
  if (node.id === id) return node
  for (const child of node.children ?? []) {
    const found = findNode(child, id)
    if (found) return found
  }
  return undefined
}

function findNodeByText(node, text) {
  if (node.text === text) return node
  for (const child of node.children ?? []) {
    const found = findNodeByText(child, text)
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

async function* streamChunks(chunks) {
  for (const chunk of chunks) {
    await delay(0)
    yield chunk
  }
}

async function main() {
  function TickingLabel() {
    const [text, setText] = React.useState('Waiting for timer')
    React.useEffect(() => {
      const timer = setTimeout(() => setText('Timer updated'), 0)
      return () => clearTimeout(timer)
    }, [])
    return React.createElement('label', { text })
  }

  const handle = render(
    React.createElement(TickingLabel),
    { headless: true, cols: 50, rows: 14, idPrefix: 'render' },
  )

  const label = await waitFor(() => findNode(handle.host.snapshot().tree, 'render-1'))
  assert.equal(label.name, 'Label')
  assert.equal(handle.host.listWindows().length, 1)
  assert.equal(handle.windowIds().length, 1)
  await waitFor(() => findNode(handle.host.snapshot().tree, 'render-1')?.text === 'Timer updated')

  let promiseRan = false
  Promise.resolve().then(() => {
    promiseRan = true
  })
  await delay(0)
  assert.equal(promiseRan, true)

  function StreamingLabel() {
    const [text, setText] = React.useState('Streaming:')
    React.useEffect(() => {
      let cancelled = false
      ;(async () => {
        let next = 'Streaming:'
        for await (const chunk of streamChunks([' one', ' two', ' done'])) {
          if (cancelled) return
          next += chunk
          setText(next)
        }
      })()
      return () => {
        cancelled = true
      }
    }, [])
    return React.createElement('label', { text })
  }

  const streamHandle = render(
    React.createElement(StreamingLabel),
    { headless: true, cols: 50, rows: 14, idPrefix: 'stream' },
  )
  await waitFor(() => findNodeByText(streamHandle.host.snapshot().tree, 'Streaming: one two done'))
  streamHandle.stop()

  const multiHandle = render(
    React.createElement(React.Fragment, null,
      React.createElement(Window, { title: 'Left', rect: [1, 1, 20, 6] },
        React.createElement('label', { text: 'Left window' })),
      React.createElement(Window, { title: 'Right', rect: [24, 1, 22, 6] },
        React.createElement('label', { text: 'Right window' }))),
    { singleWindow: false, headless: true, cols: 60, rows: 16, idPrefix: 'multi-render' },
  )
  await waitFor(() => multiHandle.host.listWindows().length === 2)
  assert.deepEqual(multiHandle.host.listWindows().map((window) => window.title), ['Left', 'Right'])
  assert.equal(multiHandle.windowIds().length, 2)
  await waitFor(() => findNodeByText(multiHandle.host.snapshot().tree, 'Right window'))
  multiHandle.stop()

  function ClickCounter() {
    const [count, setCount] = React.useState(0)
    return React.createElement('button', {
      label: `Count: ${count}`,
      onClick() {
        setCount((current) => current + 1)
      },
    })
  }

  const eventHandle = render(
    React.createElement(ClickCounter),
    { headless: true, cols: 50, rows: 14, idPrefix: 'event-render' },
  )
  await waitFor(() => findNode(eventHandle.host.snapshot().tree, 'event-render-1')?.text === 'Count: 0')
  eventHandle.host.sendEvent(eventHandle.windowId, { type: 'key', key: 'enter' })
  await waitFor(() => findNode(eventHandle.host.snapshot().tree, 'event-render-1')?.text === 'Count: 1')
  eventHandle.stop()

  function LegacyApprovalInference() {
    const store = useChatMessages([
      {
        id: 1,
        role: 'assistant',
        status: 'complete',
        blocks: [
          {
            type: 'tool_use',
            block_id: 101,
            call_id: 'call-legacy-approval',
            name: 'bash',
            input: { text: 'cargo test' },
            status: 'pending',
            approval: {
              id: 'approval-legacy',
              prompt: 'Run command?',
              options: [{ id: 'no_thanks', label: 'No thanks' }],
            },
          },
        ],
      },
    ])
    const didResolve = React.useRef(false)
    React.useEffect(() => {
      if (didResolve.current) return
      didResolve.current = true
      store.resolveApproval(101, 'no_thanks')
    }, [store])
    const tool = store.messages[0].blocks[0]
    const approval = tool.approval
    return React.createElement('label', {
      text: `Legacy approval: ${tool.status} ${approval.resolved_action ?? 'none'} ${approval.resolved_level ?? 'none'}`,
    })
  }

  const approvalHandle = render(
    React.createElement(LegacyApprovalInference),
    { headless: true, cols: 50, rows: 14, idPrefix: 'approval-render' },
  )
  await waitFor(() => findNodeByText(approvalHandle.host.snapshot().tree, 'Legacy approval: canceled deny once'))
  approvalHandle.stop()

  handle.stop()
  handle.stop()
  assert.deepEqual(handle.host.listWindows(), [])
}

main().catch((error) => {
  console.error(error)
  process.exitCode = 1
})
