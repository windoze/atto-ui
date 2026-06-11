'use strict'

const assert = require('node:assert/strict')
const {
  Button,
  ChatInputMode,
  ChatInputPanel,
  ChatMessage,
  ChatMessageList,
  ChatNoticeBlock,
  ChatPlanBlock,
  ChatTaskBlock,
  ChatTaskTranscriptItem,
  ChatTextMessage,
  ChatThinkingBlock,
  ChatToolCallMessage,
  ChatToolJsonInput,
  FileTree,
  FileTreeNode,
  Grid,
  ListBox,
  MarkdownViewer,
  RichText,
  TableView,
  TabView,
  TextBox,
  Text,
  TextSpan,
  TerminalEmulator,
  VStack,
  child,
  component,
  tab,
} = require('..')

const root = VStack({ id: 'root', spacing: 2, padding: 1 }, [
  Text('Hello', { id: 'hello', selectable: true }),
  Button({ id: 'send', text: 'Send', onClick: 'atto:callback:1', disabled: false }),
  child(Grid({ columns: 2, rowGap: 1, columnGap: 3 }, [Text('Nested')]), {
    layout: { width: 'fill' },
    meta: { slot: 'main' },
  }),
])

assert.deepStrictEqual(root, {
  type: 'VStack',
  id: 'root',
  props: { spacing: 2, padding: 1 },
  children: [
    { type: 'Text', id: 'hello', props: { text: 'Hello', selectable: true } },
    {
      type: 'Button',
      id: 'send',
      props: { label: 'Send', enabled: true },
      events: { click: 'atto:callback:1' },
    },
    {
      node: {
        type: 'Grid',
        props: { columns: 2, row_gap: 1, column_gap: 3 },
        children: [{ type: 'Text', props: { text: 'Nested' } }],
      },
      layout: { width: 'fill' },
      meta: { slot: 'main' },
    },
  ],
})

assert.deepStrictEqual(
  component('CustomWidget', {
    id: 'custom',
    props: { title: 'Raw', omitted: undefined },
    events: { activate: 'atto:callback:2', skipped: undefined },
  }),
  {
    type: 'CustomWidget',
    id: 'custom',
    props: { title: 'Raw' },
    events: { activate: 'atto:callback:2' },
  },
)

assert.deepStrictEqual(Button({ label: 'Explicit', events: { click: 'atto:callback:4' } }), {
  type: 'Button',
  props: { label: 'Explicit' },
  events: { click: 'atto:callback:4' },
})

assert.deepStrictEqual(
  RichText([TextSpan('A', { bold: true }), TextSpan('B', { href: 'https://example.test' })]),
  {
    type: 'RichText',
    children: [
      { type: 'TextSpan', props: { text: 'A', bold: true } },
      { type: 'TextSpan', props: { text: 'B', href: 'https://example.test' } },
    ],
  },
)

assert.deepStrictEqual(
  TabView({ selection: 1 }, [tab('One', Text('First')), tab('Two', Text('Second'))]),
  {
    type: 'TabView',
    props: { selection: 1 },
    children: [
      { node: { type: 'Text', props: { text: 'First' } }, meta: { title: 'One' } },
      { node: { type: 'Text', props: { text: 'Second' } }, meta: { title: 'Two' } },
    ],
  },
)

assert.deepStrictEqual(MarkdownViewer('# Title', { id: 'doc', wrapWidth: 60, onLink: 'atto:callback:5' }), {
  type: 'MarkdownViewer',
  id: 'doc',
  props: { markdown: '# Title', wrap_width: 60 },
  events: { link: 'atto:callback:5' },
})

assert.deepStrictEqual(TerminalEmulator({ command: 'sh', args: ['-lc', 'true'], captureOnClick: true, onClose: 'atto:callback:6' }), {
  type: 'TerminalEmulator',
  props: { command: 'sh', args: ['-lc', 'true'], capture_on_click: true },
  events: { close: 'atto:callback:6' },
})

const fileTreeNodes = [
  FileTreeNode(1, 'src', { kind: 'directory', expanded: true, children: [FileTreeNode(2, 'main.rs', { kind: 'file' })] }),
]
assert.deepStrictEqual(FileTree({ title: 'Files', nodes: fileTreeNodes, selection: 2, onSelect: 'atto:callback:7' }), {
  type: 'FileTree',
  props: { title: 'Files', nodes: fileTreeNodes, selection: 2 },
  events: { select: 'atto:callback:7' },
})

// border + file-type icon mapping (string and {glyph,color}) flow into props.
assert.deepStrictEqual(
  FileTree({
    border: false,
    icons: { rs: { glyph: '', color: '#ff8800' }, md: 'M' },
  }),
  {
    type: 'FileTree',
    props: {
      border: false,
      icons: { rs: { glyph: '', color: '#ff8800' }, md: 'M' },
    },
  },
)

assert.deepStrictEqual(ListBox({ items: ['a'], border: false }).props.border, false)
assert.deepStrictEqual(TableView({ rows: [['a']], border: false }).props.border, false)
assert.deepStrictEqual(TextBox({ border: false }).props.border, false)

const chatMessage = ChatTextMessage(1, 'hello', { role: 'user', timestamp: '2026-06-07T00:00:00Z' })
assert.deepStrictEqual(chatMessage, {
  id: 1,
  role: 'user',
  status: 'complete',
  meta: { timestamp: '2026-06-07T00:00:00Z' },
  blocks: [{ type: 'text', block_id: 1001, markdown: 'hello' }],
})

const multiBlockMessage = ChatMessage(2, [
  ChatThinkingBlock(2001, 'checking tools', { collapsed: true }),
  ChatPlanBlock(2003, [{ text: 'write tests' }], { decision: 'pending' }),
  ChatTaskBlock(2004, 'subagent', {
    status: 'running',
    summary: 'searching',
    transcript: [ChatTaskTranscriptItem('assistant', [ChatTextMessage(20, 'nested').blocks[0]])],
    collapsed: true,
  }),
  ChatNoticeBlock(2002, 'warning', 'context compacted'),
], { role: 'custom:agent', meta: { model: 'atto-test', usage: { input: 12, output: 34 }, elapsed_ms: 56, stop_reason: 'tool_use' } })
assert.deepStrictEqual(multiBlockMessage, {
  id: 2,
  role: 'custom:agent',
  status: 'complete',
  meta: { model: 'atto-test', usage: { input: 12, output: 34 }, elapsed_ms: 56, stop_reason: 'tool_use' },
  blocks: [
    { type: 'thinking', block_id: 2001, markdown: 'checking tools', collapsed: true },
    { type: 'plan', block_id: 2003, items: [{ text: 'write tests' }], decision: 'pending' },
    {
      type: 'task',
      block_id: 2004,
      title: 'subagent',
      status: 'running',
      summary: 'searching',
      transcript: [{ role: 'assistant', blocks: [{ type: 'text', block_id: 20001, markdown: 'nested' }] }],
      collapsed: true,
    },
    { type: 'notice', block_id: 2002, level: 'warning', text: 'context compacted' },
  ],
})

assert.deepStrictEqual(ChatToolCallMessage(3, 'bash', {
  input: ChatToolJsonInput({ command: 'cargo test' }),
  output: 'ok',
  outputKind: 'markdown',
  toolStatus: 'done',
}), {
  id: 3,
  role: 'assistant',
  status: 'complete',
  blocks: [
    { type: 'tool_use', block_id: 3001, call_id: 'tool-3', name: 'bash', input: { json: { command: 'cargo test' } }, status: 'done' },
    { type: 'tool_result', block_id: 3002, call_id: 'tool-3', ok: true, output: { markdown: 'ok' } },
  ],
})

assert.deepStrictEqual(ChatMessageList({ messages: [chatMessage], autoScroll: true, onOpenArtifact: 'atto:callback:8', onPlanDecision: 'atto:callback:10' }), {
  type: 'ChatMessageList',
  props: { messages: [chatMessage], auto_scroll: true },
  events: { open_artifact: 'atto:callback:8', plan_decision: 'atto:callback:10' },
})

const choiceMode = ChatInputMode('choice', { title: 'Pick one', options: ['A', 'B'] })
assert.deepStrictEqual(choiceMode, {
  type: 'choice',
  title: 'Pick one',
  prompt: 'Pick one',
  options: ['A', 'B'],
})

assert.deepStrictEqual(ChatInputPanel({ mode: choiceMode, draft: 'A', onSubmit: 'atto:callback:9' }), {
  type: 'ChatInputPanel',
  props: { mode: choiceMode, draft: 'A' },
  events: { submit: 'atto:callback:9' },
})
