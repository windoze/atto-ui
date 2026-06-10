import native = require('./native')

type NativeBinding = {
  readonly AppHost: AppHostConstructor
  readonly registerAllRuntimeComponents: () => void
  readonly version: () => string
}

const binding = native as unknown as NativeBinding

/** Native AppHost constructor with typed atto-ui spec and event shapes. */
export const AppHost: AppHostConstructor = binding.AppHost

/** Register built-in runtime components before constructing specs manually. */
export const registerAllRuntimeComponents: () => void = binding.registerAllRuntimeComponents

/** Return the native binding package version. */
export const version: () => string = binding.version

/** Rectangle object used by windows, layout snapshots, and rect component values. */
export interface Rect {
  readonly x: number
  readonly y: number
  readonly width: number
  readonly height: number
}

export type RectTuple = readonly [number, number, number, number]
export type RectLike = Rect | RectTuple

export type ComponentValueTag = 'bytes' | 'list' | 'map' | 'rect' | 'string_list' | 'table'

export type TaggedComponentValue =
  | { readonly $type: 'bytes'; readonly data: readonly number[] | Uint8Array }
  | { readonly $type: 'list'; readonly data: readonly ComponentValue[] }
  | { readonly $type: 'map'; readonly data: ComponentValueMap }
  | { readonly $type: 'rect'; readonly data: RectLike }
  | { readonly $type: 'string_list'; readonly data: readonly string[] }
  | { readonly $type: 'table'; readonly data: readonly (readonly string[])[] }

export interface ComponentValueMap {
  readonly [key: string]: ComponentValue
}

/** JavaScript-facing value shape accepted by runtime component props and event payloads. */
export type ComponentValue =
  | null
  | boolean
  | number
  | string
  | readonly string[]
  | readonly (readonly string[])[]
  | Uint8Array
  | Rect
  | readonly ComponentValue[]
  | ComponentValueMap
  | TaggedComponentValue

export type ComponentProps = Readonly<Record<string, ComponentValue>>
export type ComponentEvents = Readonly<Record<string, string>>

/** Runtime component spec consumed by `addDynamicWindow` and `set_tree` ops. */
export interface ComponentSpec {
  readonly type: string
  readonly type_name?: string
  readonly id?: string
  readonly props?: ComponentProps
  readonly events?: ComponentEvents
  readonly children?: readonly ComponentSpecChild[]
}

export type ComponentSpecChild =
  | ComponentSpec
  | {
      readonly node: ComponentSpec
      readonly layout?: LayoutSpec
      readonly meta?: ComponentProps
    }

export type SizeSpec =
  | 'fill'
  | 'content'
  | number
  | { readonly fixed: number }
  | { readonly weight: number }
  | { readonly fill: true }
  | { readonly content: true }

export type EdgeInsetsSpec =
  | number
  | readonly [number, number, number, number]
  | {
      readonly top?: number
      readonly right?: number
      readonly bottom?: number
      readonly left?: number
    }

export type AlignSpec = 'start' | 'center' | 'end' | 'stretch'

export type AnchorSpec =
  | 'top_left'
  | 'top_right'
  | 'bottom_left'
  | 'bottom_right'
  | 'top'
  | 'bottom'
  | 'left'
  | 'right'
  | 'center'

export interface AnchorPlacementSpec {
  readonly anchor: AnchorSpec
  readonly offset_x?: number
  readonly offset_y?: number
}

export interface LayoutSpec {
  readonly width?: SizeSpec
  readonly height?: SizeSpec
  readonly margin?: EdgeInsetsSpec
  readonly align_x?: AlignSpec
  readonly align_y?: AlignSpec
  readonly anchor?: AnchorPlacementSpec
  readonly tab_index?: number
}

/** Incremental runtime tree operations accepted by `AppHost.applyTreeOps`. */
export type TreeOp =
  | { readonly op: 'set_tree'; readonly tree: ComponentSpec }
  | {
      readonly op: 'insert'
      readonly parent_id: string
      readonly index: number
      readonly child: ComponentSpecChild
    }
  | {
      readonly op: 'insert_before'
      readonly parent_id: string
      readonly anchor_id?: string | null
      readonly child: ComponentSpecChild
    }
  | { readonly op: 'remove'; readonly id: string }
  | { readonly op: 'replace'; readonly id: string; readonly node: ComponentSpecChild }
  | {
      readonly op: 'move'
      readonly id: string
      readonly new_parent_id: string
      readonly index: number
    }
  | {
      readonly op: 'set_prop'
      readonly id: string
      readonly name: string
      readonly value: ComponentValue
    }
  | {
      readonly op: 'clear_prop'
      readonly id: string
      readonly name: string
    }
  | {
      readonly op: 'bind_event'
      readonly id: string
      readonly event: string
      readonly callback: string
    }
  | { readonly op: 'clear_event'; readonly id: string; readonly event: string }

export interface CallbackInvocation {
  readonly callbackId: string
  readonly targetId: string | null
  readonly event: string
  readonly payload: ComponentValue | null
}

export interface AppHostConfig {
  readonly headless?: boolean
  readonly cols?: number
  readonly rows?: number
  readonly tickRate?: number
  readonly mouseCapture?: boolean
  readonly hideCursor?: boolean
  readonly bracketedPaste?: boolean
  readonly keyboardEnhancement?: boolean
}

export interface AppHostConstructor {
  new (config?: AppHostConfig | null): AppHost
}

/** Typed wrapper for the native runtime host exposed by napi-rs. */
export interface AppHost {
  addDynamicWindow(title: string, rect: RectLike, root: ComponentSpec): string
  applyTreeOps(windowId: string, ops: TreeOp | readonly TreeOp[]): boolean
  step(): boolean
  dispose(): void
  drainCallbacks(): CallbackInvocation[]
  allocCallback(): string
  releaseCallback(callbackId: string): boolean
  sendEvent(windowId: string, event: InputEvent): DesktopEventResult
  closeWindow(windowId: string): boolean
  focusWindow(windowId: string): boolean
  moveWindow(windowId: string, x: number, y: number): boolean
  resizeWindow(windowId: string, width: number, height: number): boolean
  minimizeWindow(windowId: string): boolean
  restoreWindow(windowId: string): boolean
  /** Toggle maximize for a window. Returns true when the state changed. */
  maximizeWindow(windowId: string): boolean
  /** Drain window lifecycle events caused by user interaction inside the TUI. */
  drainWindowEvents(): WindowEvent[]
  listWindows(): WindowInfo[]
  setTitle(windowId: string, title: string): boolean
  setMenuBar(spec: MenuBarSpec): void
  setStatusBar(left?: string | null, right?: string | null): void
  setProperty(id: string, name: string, value: ComponentValue): void
  getProperty(id: string, name: string): ComponentValue
  snapshot(): DesktopSnapshot
  setTheme(name: ThemeName): void
  loadTheme(path: string, base?: ThemeName | null): void
  schemas(): ComponentSchema[]
}

export interface MenuBarSpec {
  readonly menus: readonly MenuSpec[]
}

export interface MenuSpec {
  readonly id?: string | null
  readonly title: string
  readonly items?: readonly MenuItemSpec[]
}

export interface MenuItemSpec {
  readonly id?: string | null
  readonly label: string
  readonly shortcut?: string | null
  readonly enabled?: boolean
  readonly callback?: string | null
  readonly items?: readonly MenuItemSpec[]
}

export type ThemeName = 'dark' | 'light' | 'turbo'

export type KnownKeyName =
  | 'backspace'
  | 'enter'
  | 'return'
  | 'left'
  | 'right'
  | 'up'
  | 'down'
  | 'home'
  | 'end'
  | 'pageup'
  | 'pagedown'
  | 'tab'
  | 'backtab'
  | 'delete'
  | 'del'
  | 'insert'
  | 'ins'
  | 'esc'
  | 'escape'
  | `f${number}`

export type KeyName = KnownKeyName | (string & {})
export type KeyModifier = 'shift' | 'control' | 'ctrl' | 'alt' | 'option' | 'super' | 'cmd' | 'command' | 'hyper' | 'meta' | 'none'
export type KeyModifiers = KeyModifier | readonly KeyModifier[]
export type KeyEventKind = 'press' | 'down' | 'release' | 'up' | 'repeat'

export type KeyEvent =
  | {
      readonly type: 'key'
      readonly key: KeyName
      readonly modifiers?: KeyModifiers | null
      readonly kind?: KeyEventKind
    }
  | {
      readonly type: 'key'
      readonly char: string
      readonly modifiers?: KeyModifiers | null
      readonly kind?: KeyEventKind
    }

export type MouseButton = 'left' | 'right' | 'middle'
export type MouseEventKind = 'down' | 'up' | 'drag' | 'move' | 'moved' | 'scrollup' | 'scrolldown' | 'scrollleft' | 'scrollright'

export interface MouseEvent {
  readonly type: 'mouse'
  readonly kind: MouseEventKind
  readonly button?: MouseButton
  readonly x?: number
  readonly y?: number
  readonly column?: number
  readonly row?: number
  readonly modifiers?: KeyModifiers | null
}

export interface PasteEvent {
  readonly type: 'paste'
  readonly text: string
}

export interface ResizeEvent {
  readonly type: 'resize'
  readonly cols: number
  readonly rows: number
}

export type FocusEvent =
  | { readonly type: 'focus_gained' | 'focusGained' }
  | { readonly type: 'focus_lost' | 'focusLost' }
  | { readonly event: 'focus_gained' | 'focusGained' }
  | { readonly event: 'focus_lost' | 'focusLost' }

export type InputEvent = KeyName | KeyEvent | MouseEvent | PasteEvent | ResizeEvent | FocusEvent

export interface DesktopEventResult {
  readonly consumed: boolean
  readonly outcome: string
  readonly action: null | { readonly type: 'close_window'; readonly windowId: string }
}

export interface WindowInfo {
  readonly id: string
  readonly tag: string | null
  readonly title: string
  readonly kind: string
  readonly state: string
  readonly rect: Rect
  readonly isFocused: boolean
}

/** A window lifecycle change originating from user interaction inside the TUI. */
export interface WindowEvent {
  readonly windowId: string
  readonly type: 'closed' | 'minimized' | 'maximized' | 'restored'
  readonly state: string | null
}

export interface DesktopSnapshot {
  readonly bounds: Rect
  readonly tree: DesktopSnapshotNode
}

export type DesktopSnapshotNodeKind = 'desktop' | 'menu_bar' | 'menu' | 'menu_item' | 'status_bar' | 'window' | 'component'

export interface DesktopSnapshotNode {
  readonly kind: DesktopSnapshotNodeKind
  readonly id: string | null
  readonly tag: string | null
  readonly name: string
  readonly typeName: string
  readonly bounds: Rect | null
  readonly text: string | null
  readonly state: string | null
  readonly windowId: string | null
  readonly properties: ComponentValueMap
  readonly children: readonly DesktopSnapshotNode[]
}

export type ComponentValueType =
  | 'Bool'
  | 'I64'
  | 'U64'
  | 'F64'
  | 'String'
  | 'StringList'
  | 'Table'
  | 'Rect'
  | 'Bytes'
  | 'List'
  | 'Map'
  | 'Unknown'

export interface PropertyMeta {
  readonly name: string
  readonly value_type: ComponentValueType
  readonly readable: boolean
  readonly writable: boolean
}

export interface ActionMeta {
  readonly name: string
  readonly payload: ComponentValueType | null
}

export interface EventMeta {
  readonly name: string
  readonly payload: ComponentValueType | null
}

export interface ComponentSchema {
  readonly type_name: string
  readonly properties: readonly PropertyMeta[]
  readonly actions: readonly ActionMeta[]
  readonly events: readonly EventMeta[]
  readonly allows_children: boolean
}

export * from './src/builders'
