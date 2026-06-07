import {
  AppHost,
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

type _CallbacksAreTyped = AssertFalse<IsAny<typeof callbacks>>
type _WindowsAreTyped = AssertFalse<IsAny<typeof windows>>
type _SnapshotIsTyped = AssertFalse<IsAny<typeof snapshot>>

void changed
void callbacks
void released
void windows
void snapshot
