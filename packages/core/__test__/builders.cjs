'use strict'

const assert = require('node:assert/strict')
const {
  Button,
  ChatInputMode,
  ChatInputPanel,
  ChatMessageList,
  ChatTextMessage,
  FileTree,
  FileTreeNode,
  Grid,
  MarkdownViewer,
  RichText,
  TabView,
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

const chatMessage = ChatTextMessage(1, 'hello', { sender: 'user', timestamp: '2026-06-07T00:00:00Z' })
assert.deepStrictEqual(chatMessage, {
  id: 1,
  sender: 'user',
  timestamp: '2026-06-07T00:00:00Z',
  status: 'final',
  content: { markdown: 'hello' },
})

assert.deepStrictEqual(ChatMessageList({ messages: [chatMessage], autoScroll: true, onOpenArtifact: 'atto:callback:8' }), {
  type: 'ChatMessageList',
  props: { messages: [chatMessage], auto_scroll: true },
  events: { open_artifact: 'atto:callback:8' },
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
