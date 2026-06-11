import {
  AppHost,
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
  MarkdownViewer,
  Text,
  TerminalEmulator,
  VStack,
  child,
  tab,
  type CallbackInvocation,
  type ComponentSpec,
  type ComponentValue,
  type DesktopSnapshot,
  type MenuBarSpec,
  type TreeOp,
  type WindowInfo,
} from '..'

type IsAny<T> = 0 extends 1 & T ? true : false
type AssertFalse<T extends false> = T

const root: ComponentSpec = {
  type: 'VStack',
  id: 'root',
  children: [{ type: 'Label', id: 'title', props: { text: 'Before' } }],
}

const values: readonly ComponentValue[] = [
  null,
  true,
  1,
  'text',
  ['a', 'b'],
  [['a', 'b']],
  { x: 1, y: 2, width: 3, height: 4 },
  { $type: 'bytes', data: [1, 2, 3] },
]

const ops: readonly TreeOp[] = [
  { op: 'set_tree', tree: root },
  { op: 'insert', parent_id: 'root', index: 0, child: { type: 'Button', id: 'ok' } },
  { op: 'insert_before', parent_id: 'root', anchor_id: 'title', child: { type: 'Label', id: 'pre' } },
  { op: 'insert_before', parent_id: 'root', anchor_id: null, child: { type: 'Label', id: 'tail' } },
  { op: 'set_prop', id: 'title', name: 'text', value: values[3] },
  { op: 'clear_prop', id: 'title', name: 'text' },
  { op: 'bind_event', id: 'ok', event: 'click', callback: 'atto:callback:1' },
  { op: 'clear_event', id: 'ok', event: 'click' },
]

const menu: MenuBarSpec = {
  menus: [{ title: 'File', items: [{ label: 'Open', shortcut: 'Ctrl+O', callback: 'atto:callback:1' }] }],
}

const host = new AppHost({ headless: true, cols: 40, rows: 12, tickRate: 0 })
const windowId: string = host.addDynamicWindow('Typed', [0, 0, 20, 6], root)
const changed: boolean = host.applyTreeOps(windowId, ops)
host.setMenuBar(menu)
host.setStatusBar('Ready', 'Ln 1')
const callbacks: CallbackInvocation[] = host.drainCallbacks()
const released: boolean = host.releaseCallback(host.allocCallback())
const windows: WindowInfo[] = host.listWindows()
const snapshot: DesktopSnapshot = host.snapshot()

const builtRoot = VStack({ id: 'built-root', padding: 1 }, [
  child(Text('Hello', { id: 'built-text', selectable: true }), {
    layout: { width: 'fill' },
    meta: { title: 'Greeting' },
  }),
  Button({ id: 'built-button', label: 'OK', onClick: 'atto:callback:2' }),
  Grid({ columns: 2, rowGap: 1 }, [tab('Tab label', Text('Cell'))]),
])
const builtSpec: ComponentSpec = builtRoot
const shortcutButton: ComponentSpec = Button('Send', { onClick: 'atto:callback:3' })
const markdownSpec: ComponentSpec = MarkdownViewer('**doc**', { onLink: 'atto:callback:4' })
const terminalSpec: ComponentSpec = TerminalEmulator({ command: 'sh', args: ['-lc', 'true'], onInput: 'atto:callback:5' })
const fileTreeSpec: ComponentSpec = FileTree({
  title: 'Files',
  nodes: [FileTreeNode(1, 'src', { kind: 'directory', children: [FileTreeNode(2, 'main.rs')] })],
  onSelect: 'atto:callback:6',
})
const chatMode: ComponentValue = ChatInputMode('choice', { options: ['yes', 'no'] })
const chatMessage = ChatMessage(2, [
  ChatThinkingBlock(2001, 'thinking', { collapsed: true }),
  ChatPlanBlock(2003, [{ text: 'review plan' }], { decision: 'pending' }),
  ChatTaskBlock(2004, 'subagent', {
    status: 'running',
    summary: 'searching',
    transcript: [ChatTaskTranscriptItem('assistant', [ChatTextMessage(20, 'nested').blocks[0]])],
  }),
  ChatNoticeBlock(2002, 'info', 'ready'),
], { role: 'custom:agent', meta: { model: 'atto-test', usage: { input: 1, output: 2 } } })
const chatToolMessage = ChatToolCallMessage(3, 'bash', {
  input: ChatToolJsonInput({ command: 'cargo test' }),
  output: 'ok',
  toolStatus: 'done',
})
const chatListSpec: ComponentSpec = ChatMessageList({
  messages: [ChatTextMessage(1, 'hello', { role: 'user' }), chatMessage, chatToolMessage],
  onLoadMore: 'atto:callback:7',
  onPlanDecision: 'atto:callback:9',
})
const chatInputSpec: ComponentSpec = ChatInputPanel({ mode: ChatInputMode(), onSubmit: 'atto:callback:8' })

// @ts-expect-error callback handles must be strings from AppHost.allocCallback().
Button({ label: 'Bad', onClick: 1 })
// @ts-expect-error Text content must be a string.
Text(123)
// @ts-expect-error file tree node ids must be numeric runtime ids.
FileTreeNode('bad', 'name')

type _CallbacksAreTyped = AssertFalse<IsAny<typeof callbacks>>
type _WindowsAreTyped = AssertFalse<IsAny<typeof windows>>
type _SnapshotIsTyped = AssertFalse<IsAny<typeof snapshot>>
type _BuilderIsTyped = AssertFalse<IsAny<typeof builtRoot>>

void changed
void callbacks
void released
void windows
void snapshot
void builtSpec
void shortcutButton
void markdownSpec
void terminalSpec
void fileTreeSpec
void chatMode
void chatMessage
void chatToolMessage
void chatListSpec
void chatInputSpec
