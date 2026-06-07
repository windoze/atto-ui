#!/usr/bin/env node

// End-to-end Node/React example for atto-ui state, events, controlled input, and streaming updates.

const { createRequire } = require('node:module')
const { join } = require('node:path')
const { setTimeout: delay } = require('node:timers/promises')

const localReactPackage = join(__dirname, '..', '..', 'packages', 'react', 'package.json')
const localReactRequire = createRequire(localReactPackage)
const React = requireFromInstalledOrLocal('react', () => localReactRequire('react'))
const {
  Button,
  ListBox,
  Menu,
  MenuBar,
  MenuItem,
  StatusBar,
  TextBox,
  VStack,
  Window,
  render,
} = requireFromInstalledOrLocal('@atto-ui/react', () => require('../../packages/react/dist'))

const h = React.createElement

function requireFromInstalledOrLocal(name, fallback) {
  try {
    return require(name)
  } catch (error) {
    if (error && error.code !== 'MODULE_NOT_FOUND') throw error
    return fallback()
  }
}

// Parse CLI flags and environment variables without adding an argument parser dependency.
function parseOptions(argv, env) {
  const options = {
    autostart: env.ATTO_UI_EXAMPLE_AUTOSTART !== '0',
    headless: env.ATTO_UI_EXAMPLE_HEADLESS === '1',
    mockDelayMs: Number(env.ATTO_UI_EXAMPLE_TOKEN_DELAY_MS ?? 8),
    prompt: env.ATTO_UI_CHAT_PROMPT ?? 'Summarize how atto-ui keeps React state and terminal input responsive.',
    provider: env.ATTO_UI_CHAT_PROVIDER ?? 'mock',
  }

  for (const arg of argv) {
    if (arg === '--headless') options.headless = true
    else if (arg === '--no-autostart') options.autostart = false
    else if (arg === '--fast') options.mockDelayMs = 0
    else if (arg.startsWith('--provider=')) options.provider = arg.slice('--provider='.length)
    else if (arg.startsWith('--prompt=')) options.prompt = arg.slice('--prompt='.length)
  }

  return options
}

// Render the two-window demo app and wire UI events into React state.
function AgentChatExample({ autostart, initialPrompt, mockDelayMs, onStreamDone, provider }) {
  const [count, setCount] = React.useState(0)
  const [todoDraft, setTodoDraft] = React.useState('')
  const [todos, setTodos] = React.useState(['Build binding', 'Render React UI'])
  const [selectedTodo, setSelectedTodo] = React.useState(0)
  const [prompt, setPrompt] = React.useState(initialPrompt)
  const [messages, setMessages] = React.useState([
    { id: 'system-1', role: 'system', text: 'Choose a provider, edit the prompt, then stream a reply.' },
  ])
  const [isStreaming, setIsStreaming] = React.useState(false)
  const [status, setStatus] = React.useState('ready')
  const streamIdRef = React.useRef(0)
  const nextMessageIdRef = React.useRef(1)

  React.useEffect(() => {
    if (autostart) {
      startChat(initialPrompt)
    }
    return () => {
      streamIdRef.current += 1
    }
  }, [])

  function addTodo() {
    const label = todoDraft.trim() || `Task ${todos.length + 1}`
    setTodos((current) => [...current, label])
    setSelectedTodo(todos.length)
    setTodoDraft('')
    setStatus(`added todo: ${label}`)
  }

  function removeSelectedTodo() {
    if (todos.length === 0) {
      setStatus('todo list already empty')
      return
    }
    const removeIndex = Math.min(selectedTodo, todos.length - 1)
    const removed = todos[removeIndex]
    const nextTodos = todos.filter((_, index) => index !== removeIndex)
    setTodos(nextTodos)
    setSelectedTodo(Math.max(0, Math.min(removeIndex, nextTodos.length - 1)))
    setStatus(`removed todo: ${removed}`)
  }

  function resetDemo() {
    streamIdRef.current += 1
    setCount(0)
    setTodoDraft('')
    setTodos(['Build binding', 'Render React UI'])
    setSelectedTodo(0)
    setPrompt(initialPrompt)
    setMessages([
      { id: 'system-1', role: 'system', text: 'Choose a provider, edit the prompt, then stream a reply.' },
    ])
    setIsStreaming(false)
    setStatus('reset')
  }

  async function startChat(nextPrompt = prompt) {
    const request = nextPrompt.trim()
    if (request.length === 0 || isStreaming) return

    const streamId = streamIdRef.current + 1
    streamIdRef.current = streamId
    const userId = nextMessageId('user')
    const assistantId = nextMessageId('assistant')
    let finalText = ''

    setIsStreaming(true)
    setStatus(`streaming with ${provider}`)
    setMessages((current) => [
      ...current,
      { id: userId, role: 'user', text: request },
      { id: assistantId, role: 'assistant', streamId, text: '' },
    ])

    try {
      for await (const token of createTokenStream(provider, request, { mockDelayMs })) {
        if (streamIdRef.current !== streamId) return
        finalText += token
        setMessages((current) => updateAssistantMessage(current, streamId, finalText))
      }
      setStatus(`stream complete: ${finalText.length} chars`)
      onStreamDone?.({ ok: true, text: finalText })
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      finalText = `Stream failed: ${message}`
      setStatus('stream failed')
      setMessages((current) => updateAssistantMessage(current, streamId, finalText))
      onStreamDone?.({ ok: false, text: finalText })
    } finally {
      if (streamIdRef.current === streamId) setIsStreaming(false)
    }
  }

  function nextMessageId(prefix) {
    const id = `${prefix}-${nextMessageIdRef.current}`
    nextMessageIdRef.current += 1
    return id
  }

  const lastAssistant = [...messages].reverse().find((message) => message.role === 'assistant')
  const chatLines = messages.slice(-7).map((message) => {
    const label = message.role === 'assistant' ? 'Assistant' : message.role === 'user' ? 'You' : 'System'
    return h('label', { key: message.id, text: `${label}: ${message.text || '...'}` })
  })

  return h(
    React.Fragment,
    null,
    h(MenuBar, null,
      h(Menu, { title: 'Demo' },
        h(MenuItem, { label: 'Reset', shortcut: 'Ctrl+R', onClick: resetDemo }),
        h(MenuItem, { label: 'Stream prompt', shortcut: 'Ctrl+S', onClick: () => startChat() }))),
    h(StatusBar, {
      left: `provider=${provider} status=${status}`,
      right: 'Ctrl+Q exits',
    }),
    h(Window, { title: 'State + Events + Todos', rect: [1, 1, 36, 20] },
      h(VStack, { spacing: 1 },
        h('label', { text: `Counter: ${count}` }),
        h(Button, { onClick: () => setCount((current) => current + 1) }, 'Increment counter'),
        h(TextBox, {
          title: 'New todo',
          value: todoDraft,
          onChange(value) {
            setTodoDraft(value)
          },
          onSubmit: addTodo,
        }),
        h('label', { text: `Draft: ${todoDraft || '-'}` }),
        h(Button, { onClick: addTodo }, 'Add todo'),
        h(Button, { onClick: removeSelectedTodo }, 'Remove selected'),
        h('label', { text: `Todos: ${todos.length}` }),
        h(ListBox, {
          title: 'Todos',
          height: 5,
          items: todos,
          selectedIndex: selectedTodo,
          onSelect(index) {
            setSelectedTodo(index)
          },
        }))),
    h(Window, { title: 'Streaming Chat', rect: [39, 1, 60, 20] },
      h(VStack, { spacing: 1 },
        h('label', { text: `Provider: ${provider}` }),
        h(TextBox, {
          title: 'Prompt',
          value: prompt,
          onChange(value) {
            setPrompt(value)
          },
          onSubmit() {
            startChat()
          },
        }),
        h(Button, { enabled: !isStreaming, onClick: () => startChat() }, isStreaming ? 'Streaming...' : 'Send prompt'),
        h('label', { text: lastAssistant ? `Last reply chars: ${lastAssistant.text.length}` : 'Last reply chars: 0' }),
        ...chatLines)),
  )
}

// Replace only the assistant message owned by the active stream.
function updateAssistantMessage(messages, streamId, text) {
  return messages.map((message) => (
    message.streamId === streamId ? { ...message, text } : message
  ))
}

// Select the configured token source; real SDKs and mock streams share one async iterator shape.
async function* createTokenStream(provider, prompt, options) {
  if (provider === 'mock') {
    yield* mockTokenStream(prompt, options.mockDelayMs)
    return
  }
  if (provider === 'openai') {
    yield* openAiTokenStream(prompt)
    return
  }
  if (provider === 'anthropic') {
    yield* anthropicTokenStream(prompt)
    return
  }
  throw new Error(`unknown provider '${provider}', expected mock, openai, or anthropic`)
}

// Deterministic offline token source used by the default demo and CI smoke checks.
async function* mockTokenStream(prompt, delayMs) {
  const text = [
    'This deterministic mock stream uses the same for-await path as an LLM SDK. ',
    'The prompt was: ',
    prompt,
    '. UI input, timers, and React state keep progressing between token chunks.',
  ].join('')
  const tokens = text.match(/\S+\s*/g) ?? []
  for (const token of tokens) {
    if (delayMs > 0) await delay(delayMs)
    else await delay(0)
    yield token
  }
}

// Stream chat completion deltas from the optional OpenAI SDK.
async function* openAiTokenStream(prompt) {
  if (!process.env.OPENAI_API_KEY) {
    throw new Error('OPENAI_API_KEY is required when --provider=openai')
  }
  const module = await import('openai')
  const OpenAI = module.default ?? module.OpenAI
  const client = new OpenAI({ apiKey: process.env.OPENAI_API_KEY })
  const stream = await client.chat.completions.create({
    model: process.env.OPENAI_MODEL ?? 'gpt-4o-mini',
    messages: [{ role: 'user', content: prompt }],
    stream: true,
  })

  for await (const part of stream) {
    const token = part.choices?.[0]?.delta?.content
    if (token) yield token
  }
}

// Stream text deltas from the optional Anthropic SDK.
async function* anthropicTokenStream(prompt) {
  if (!process.env.ANTHROPIC_API_KEY) {
    throw new Error('ANTHROPIC_API_KEY is required when --provider=anthropic')
  }
  const module = await import('@anthropic-ai/sdk')
  const Anthropic = module.default ?? module.Anthropic
  const client = new Anthropic({ apiKey: process.env.ANTHROPIC_API_KEY })
  const stream = client.messages.stream({
    model: process.env.ANTHROPIC_MODEL ?? 'claude-3-5-haiku-latest',
    max_tokens: 256,
    messages: [{ role: 'user', content: prompt }],
  })

  for await (const event of stream) {
    if (event.type === 'content_block_delta' && event.delta?.type === 'text_delta') {
      yield event.delta.text
    }
  }
}

// Run the same app in headless mode and assert that state, events, controlled input, and streaming work.
async function runHeadless(options) {
  let resolveDone
  const streamDone = new Promise((resolve) => {
    resolveDone = resolve
  })
  const handle = render(h(AgentChatExample, {
    autostart: true,
    initialPrompt: options.prompt,
    mockDelayMs: options.mockDelayMs,
    onStreamDone: resolveDone,
    provider: options.provider,
  }), {
    cols: 100,
    headless: true,
    idPrefix: 'agent-chat-example',
    rows: 24,
    singleWindow: false,
  })

  try {
    await waitFor(() => handle.windowIds().length === 2, 'example windows')
    const stateWindowId = handle.windowIds()[0]
    sendKey(handle, stateWindowId, 'enter')
    await waitFor(() => hasSnapshotText(handle, 'Counter: 1'), 'counter event')
    sendKey(handle, stateWindowId, 'tab')
    sendChar(handle, stateWindowId, 'x')
    await waitFor(() => hasSnapshotText(handle, 'Draft: x'), 'controlled todo input')
    sendKey(handle, stateWindowId, 'tab')
    sendKey(handle, stateWindowId, 'enter')
    await waitFor(() => hasSnapshotText(handle, 'Todos: 3'), 'todo add event')

    const result = await withTimeout(streamDone, 10_000, 'stream completion')
    await waitFor(() => {
      const texts = collectTexts(handle.host.snapshot().tree)
      return texts.some((text) => text.includes(result.text.slice(0, 40)))
    }, 'final assistant text')

    const texts = collectTexts(handle.host.snapshot().tree)
    console.log('Atto UI React example headless snapshot:')
    for (const text of interestingSnapshotLines(texts)) {
      console.log(`- ${text}`)
    }
  } finally {
    handle.stop()
  }
}

// Send a synthetic key event through the native AppHost event path.
function sendKey(handle, windowId, key) {
  handle.host.sendEvent(windowId, { type: 'key', key })
}

// Send a synthetic character input through the native AppHost event path.
function sendChar(handle, windowId, char) {
  handle.host.sendEvent(windowId, { type: 'key', char })
}

// Check snapshot text without depending on terminal escape sequences.
function hasSnapshotText(handle, expected) {
  return collectTexts(handle.host.snapshot().tree).includes(expected)
}

// Start the interactive terminal renderer; Ctrl+Q is handled by AppHost.
function runInteractive(options) {
  render(h(AgentChatExample, {
    autostart: options.autostart,
    initialPrompt: options.prompt,
    mockDelayMs: options.mockDelayMs,
    provider: options.provider,
  }), {
    idPrefix: 'agent-chat-example',
    singleWindow: false,
  })
}

// Collect visible text from the deterministic AppHost snapshot tree.
function collectTexts(node, texts = []) {
  if (typeof node.text === 'string' && node.text.length > 0) texts.push(node.text)
  for (const child of node.children ?? []) collectTexts(child, texts)
  return texts
}

// Keep headless output compact enough to paste into task records.
function interestingSnapshotLines(texts) {
  const prefixes = ['Counter:', 'Todos:', 'Provider:', 'Last reply chars:', 'Assistant:']
  return texts.filter((text) => prefixes.some((prefix) => text.startsWith(prefix))).slice(0, 12)
}

// Poll until React commits the expected state or fail with a useful label.
async function waitFor(predicate, label) {
  const deadline = Date.now() + 1500
  while (Date.now() < deadline) {
    if (predicate()) return
    await delay(10)
  }
  throw new Error(`timed out waiting for ${label}`)
}

// Bound async waits so a broken stream cannot hang the smoke check.
async function withTimeout(promise, timeoutMs, label) {
  let timer
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(`timed out waiting for ${label}`)), timeoutMs)
      }),
    ])
  } finally {
    clearTimeout(timer)
  }
}

// CLI entrypoint shared by interactive and headless modes.
async function main() {
  const options = parseOptions(process.argv.slice(2), process.env)
  if (options.headless) {
    await runHeadless(options)
  } else {
    runInteractive(options)
  }
}

if (require.main === module) {
  main().catch((error) => {
    console.error(error)
    process.exitCode = 1
  })
}

module.exports = {
  AgentChatExample,
  createTokenStream,
  parseOptions,
  runHeadless,
}
