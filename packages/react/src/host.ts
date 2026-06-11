import type {
  AppHost,
  CallbackInvocation,
  ComponentSpec,
  ComponentSpecChild,
  ComponentValue,
  LayoutSpec,
  MenuBarSpec,
  MenuItemSpec,
  Rect,
  TreeOp,
} from '@atto-ui/core'

import { CallbackEventDispatcher } from './events'

export type RenderHost = Pick<AppHost, 'applyTreeOps' | 'allocCallback'> & {
  releaseCallback?(callbackId: string): boolean
}

export type DesktopRenderHost = RenderHost & Pick<
  AppHost,
  | 'addDynamicWindow'
  | 'closeWindow'
  | 'moveWindow'
  | 'resizeWindow'
  | 'setTitle'
  | 'setMenuBar'
  | 'setStatusBar'
  | 'minimizeWindow'
  | 'restoreWindow'
  | 'maximizeWindow'
  | 'drainWindowEvents'
>

type HostContainerMode = 'window' | 'desktop'

interface PendingTreeOp {
  readonly windowId: string
  readonly op: TreeOp
}

export interface HostContainerOptions {
  readonly idPrefix?: string
}

export interface HostContainer {
  readonly host: RenderHost
  readonly mode: HostContainerMode
  readonly windowId: string | null
  readonly idPrefix: string
  readonly rootChildren: HostInstance[]
  readonly pendingOps: PendingTreeOp[]
  readonly eventDispatcher: CallbackEventDispatcher
  nextId: number
  needsTreeFlush: boolean
  lastTree: ComponentSpec | null
}

export interface HostInstance {
  readonly id: string
  readonly type: string
  props: HostProps
  readonly events: HostEventBindings
  readonly children: HostInstance[]
  windowId: string | null
  parent: HostContainer | HostInstance | null
  needsTreeFlush: boolean
  needsDesktopSync: boolean
  controlledText: boolean
  lastTree: ComponentSpec | null
  /** Window lifecycle callbacks (onClose/onMinimize/…). Only set on Window instances. */
  windowLifecycle: WindowLifecycle
  /** Per-child layout (size/margin/anchor) applied when this node is a stack/grid child. */
  layout: LayoutSpec | null
}

export type WindowLifecycleKey = 'close' | 'minimize' | 'maximize' | 'restore'
export type WindowLifecycle = Partial<Record<WindowLifecycleKey, unknown>>

/** Maps the `on*` event name (after `eventNameFromProp`) to a lifecycle key. */
const WINDOW_LIFECYCLE_EVENTS: Readonly<Record<string, WindowLifecycleKey>> = {
  close: 'close',
  minimize: 'minimize',
  maximize: 'maximize',
  restore: 'restore',
}

/** Maps a binding window event `type` to the lifecycle callback key. */
const WINDOW_EVENT_TYPE_TO_KEY: Readonly<Record<string, WindowLifecycleKey>> = {
  closed: 'close',
  minimized: 'minimize',
  maximized: 'maximize',
  restored: 'restore',
}

export type HostProps = Readonly<Record<string, ComponentValue>>
export type HostEventBindings = Record<string, HostEventBinding>

export interface HostEventBinding {
  callbackId: string
  handler: unknown
}

export interface HostUpdatePayload {
  readonly props: HostProps
  readonly controlledText: boolean
  readonly setProps: readonly HostPropUpdate[]
  readonly clearProps: readonly string[]
  readonly bindEvents: readonly HostEventUpdate[]
  readonly clearEvents: readonly string[]
  readonly updateEvents: readonly HostEventUpdate[]
}

export interface HostPropUpdate {
  readonly name: string
  readonly value: ComponentValue
}

export interface HostEventUpdate {
  readonly event: string
  readonly handler: unknown
}

let nextContainerId = 1

const CONTROLLED_TEXT_PROP = '__attoControlledText'

/** Create the single-window container that the React reconciler mutates. */
export function createHostContainer(
  host: RenderHost,
  windowId: string,
  options: HostContainerOptions = {},
): HostContainer {
  const idPrefix = options.idPrefix ?? `atto-react-${nextContainerId++}`
  return {
    host,
    mode: 'window',
    windowId,
    idPrefix,
    rootChildren: [],
    pendingOps: [],
    eventDispatcher: new CallbackEventDispatcher({
      allocCallback: () => host.allocCallback(),
      releaseCallback: host.releaseCallback?.bind(host),
    }),
    nextId: 0,
    needsTreeFlush: false,
    lastTree: null,
  }
}

/** Create the virtual desktop container used by declarative multi-window rendering. */
export function createDesktopHostContainer(
  host: DesktopRenderHost,
  options: HostContainerOptions = {},
): HostContainer {
  const idPrefix = options.idPrefix ?? `atto-react-${nextContainerId++}`
  return {
    host,
    mode: 'desktop',
    windowId: null,
    idPrefix,
    rootChildren: [],
    pendingOps: [],
    eventDispatcher: new CallbackEventDispatcher({
      allocCallback: () => host.allocCallback(),
      releaseCallback: host.releaseCallback?.bind(host),
    }),
    nextId: 0,
    needsTreeFlush: false,
    lastTree: null,
  }
}

/** Allocate a host component instance and assign its runtime ComponentSpec id. */
export function createHostInstance(
  container: HostContainer,
  type: string,
  props: Readonly<Record<string, unknown>>,
): HostInstance {
  const normalizedType = normalizeHostType(type)
  const isWindow = normalizedType === 'Window'
  return {
    id: `${container.idPrefix}-${++container.nextId}`,
    type: normalizedType,
    props: sanitizeProps(props),
    events: createEventBindings(container, props, isWindow),
    children: [],
    windowId: null,
    parent: null,
    needsTreeFlush: false,
    needsDesktopSync: false,
    controlledText: isControlledTextProps(normalizedType, props),
    lastTree: null,
    windowLifecycle: isWindow ? extractWindowLifecycle(props) : {},
    layout: extractLayout(props),
  }
}

function extractLayout(props: Readonly<Record<string, unknown>>): LayoutSpec | null {
  const layout = props.layout
  return layout !== null && typeof layout === 'object' ? (layout as LayoutSpec) : null
}

/** Represent raw React text as a TextSpan so RichText can merge adjacent spans in Rust. */
export function createHostTextInstance(container: HostContainer, text: string): HostInstance {
  return {
    id: `${container.idPrefix}-${++container.nextId}`,
    type: 'TextSpan',
    props: text ? { text } : {},
    events: {},
    children: [],
    windowId: null,
    parent: null,
    needsTreeFlush: false,
    needsDesktopSync: false,
    controlledText: false,
    lastTree: null,
    windowLifecycle: {},
    layout: null,
  }
}

export function appendInitialChild(parent: HostInstance, child: HostInstance): void {
  attachChild(parent, child, null)
}

export function appendChild(parent: HostInstance, child: HostInstance): void {
  if (isMenuTreeInstance(parent)) {
    attachChild(parent, child, null)
    markMenuBarForSync(parent)
    return
  }

  const shouldFlushWindowRoot = isWindowInstance(parent)
  const shouldQueue = parent.windowId !== null && !shouldFlushWindowRoot
  attachChild(parent, child, null)
  if (shouldFlushWindowRoot) {
    markWindowForTreeFlush(parent)
  } else if (shouldQueue) {
    enqueueChildInsert(parent, child, null)
  } else {
    markContainerForFlush(parent)
  }
}

export function insertBefore(
  parent: HostInstance,
  child: HostInstance,
  beforeChild: HostInstance,
): void {
  if (isMenuTreeInstance(parent)) {
    attachChild(parent, child, beforeChild)
    markMenuBarForSync(parent)
    return
  }

  const shouldFlushWindowRoot = isWindowInstance(parent)
  const shouldQueue = parent.windowId !== null && !shouldFlushWindowRoot
  attachChild(parent, child, beforeChild)
  if (shouldFlushWindowRoot) {
    markWindowForTreeFlush(parent)
  } else if (shouldQueue) {
    enqueueChildInsert(parent, child, beforeChild)
  } else {
    markContainerForFlush(parent)
  }
}

export function removeChild(parent: HostInstance, child: HostInstance): void {
  if (isMenuTreeInstance(parent)) {
    releaseEventBindingsForSubtree(child)
    detachFromParent(child)
    if (child.parent === parent) {
      child.parent = null
    }
    markMenuBarForSync(parent)
    return
  }

  const shouldFlushWindowRoot = isWindowInstance(parent)
  const shouldQueue = parent.windowId !== null && child.windowId !== null && !shouldFlushWindowRoot
  if (shouldQueue) {
    enqueueClearEventsForSubtree(parent, child)
    enqueueTreeOpForInstance(parent, { op: 'remove', id: child.id })
  } else if (shouldFlushWindowRoot) {
    enqueueClearEventsForSubtree(parent, child)
    markWindowForTreeFlush(parent)
  }
  releaseEventBindingsForSubtree(child)
  detachFromParent(child)
  if (child.parent === parent) {
    child.parent = null
  }
  setSubtreeWindowId(child, null)
  if (!shouldQueue) {
    markContainerForFlush(parent)
  }
}

export function appendChildToContainer(container: HostContainer, child: HostInstance): void {
  if (container.mode === 'desktop') {
    attachRootChild(container, child, null)
    mountDesktopChild(container, child)
    return
  }

  attachRootChild(container, child, null)
  container.needsTreeFlush = true
}

export function insertInContainerBefore(
  container: HostContainer,
  child: HostInstance,
  beforeChild: HostInstance,
): void {
  if (container.mode === 'desktop') {
    attachRootChild(container, child, beforeChild)
    mountDesktopChild(container, child)
    return
  }

  attachRootChild(container, child, beforeChild)
  container.needsTreeFlush = true
}

export function removeChildFromContainer(container: HostContainer, child: HostInstance): void {
  if (container.mode === 'desktop') {
    unmountDesktopChild(container, child)
    releaseEventBindingsForSubtree(child)
    detachFromParent(child)
    if (child.parent === container) {
      child.parent = null
    }
    setSubtreeWindowId(child, null)
    return
  }

  releaseEventBindingsForSubtree(child)
  detachFromParent(child)
  if (child.parent === container) {
    child.parent = null
  }
  setSubtreeWindowId(child, null)
  container.needsTreeFlush = true
}

export function clearContainer(container: HostContainer): boolean {
  if (container.mode === 'desktop') {
    for (const child of [...container.rootChildren]) {
      unmountDesktopChild(container, child)
      releaseEventBindingsForSubtree(child)
      child.parent = null
      setSubtreeWindowId(child, null)
    }
    container.rootChildren.length = 0
    container.pendingOps.length = 0
    return false
  }

  for (const child of container.rootChildren) {
    releaseEventBindingsForSubtree(child)
    child.parent = null
    setSubtreeWindowId(child, null)
  }
  container.rootChildren.length = 0
  container.needsTreeFlush = true
  return false
}

export function detachDeletedHostInstance(instance: HostInstance): void {
  releaseEventBindingsForSubtree(instance)
}

export function updateTextInstance(textInstance: HostInstance, text: string): void {
  const oldText = typeof textInstance.props.text === 'string' ? textInstance.props.text : ''
  textInstance.props = text ? { text } : {}
  if (oldText === text) return
  if (textInstance.windowId !== null) {
    enqueueTreeOpForInstance(textInstance, {
      op: 'set_prop',
      id: textInstance.id,
      name: 'text',
      value: text,
    })
  } else {
    markContainerForFlush(textInstance)
  }
}

export function toComponentSpec(instance: HostInstance): ComponentSpec {
  if (isVirtualDesktopInstance(instance)) {
    throw new Error(`${instance.type} is a virtual desktop node and cannot be lowered to ComponentSpec`)
  }

  const spec: {
    type: string
    id: string
    props?: HostProps
    events?: Readonly<Record<string, string>>
    children?: readonly ComponentSpecChild[]
  } = {
    type: instance.type,
    id: instance.id,
  }

  if (Object.keys(instance.props).length > 0) {
    spec.props = instance.props
  }
  const events = eventsToSpec(instance.events)
  if (Object.keys(events).length > 0) {
    spec.events = events
  }
  if (instance.children.length > 0) {
    spec.children = instance.children.map(childToComponentSpec)
  }

  return spec
}

/** Lower a child to a ComponentSpecChild, wrapping it with its per-child layout when set. */
function childToComponentSpec(instance: HostInstance): ComponentSpecChild {
  const node = toComponentSpec(instance)
  return instance.layout ? { node, layout: instance.layout } : node
}

/** Flush root replacement or incremental TreeOp mutations into the target atto-ui window. */
export function flushStaticTree(container: HostContainer): void {
  if (container.mode === 'desktop') {
    flushDesktopContainer(container)
    return
  }

  const windowId = requireContainerWindowId(container)
  if (container.needsTreeFlush) {
    if (container.rootChildren.length > 1) {
      throw new Error('atto-ui React root currently requires at most one host child')
    }

    const tree = container.rootChildren.length === 0
      ? emptyRootSpec(container)
      : toComponentSpec(container.rootChildren[0])
    const op: TreeOp = { op: 'set_tree', tree }
    container.host.applyTreeOps(windowId, op)
    container.lastTree = tree
    container.pendingOps.length = 0
    container.needsTreeFlush = false
    return
  }

  flushPendingOps(container)
  container.lastTree = container.rootChildren.length === 0
    ? emptyRootSpec(container)
    : toComponentSpec(container.rootChildren[0])
}

function flushDesktopContainer(container: HostContainer): void {
  const resetWindows = new Set<string>()

  for (const child of container.rootChildren) {
    if (isWindowInstance(child) && child.needsTreeFlush && child.windowId !== null) {
      const tree = windowRootSpec(child)
      container.host.applyTreeOps(child.windowId, { op: 'set_tree', tree })
      child.lastTree = tree
      child.needsTreeFlush = false
      resetWindows.add(child.windowId)
    } else if (isMenuBarInstance(child) && child.needsDesktopSync) {
      syncMenuBarInstance(child)
    } else if (isStatusBarInstance(child) && child.needsDesktopSync) {
      syncStatusBarInstance(child)
    }
  }

  if (resetWindows.size > 0) {
    container.pendingOps.splice(
      0,
      container.pendingOps.length,
      ...container.pendingOps.filter((pending) => !resetWindows.has(pending.windowId)),
    )
  }

  flushPendingOps(container)
}

function flushPendingOps(container: HostContainer): void {
  if (container.pendingOps.length === 0) return

  const buckets = new Map<string, TreeOp[]>()
  for (const { windowId, op } of container.pendingOps) {
    const bucket = buckets.get(windowId)
    if (bucket) {
      bucket.push(op)
    } else {
      buckets.set(windowId, [op])
    }
  }

  for (const [windowId, ops] of buckets) {
    container.host.applyTreeOps(windowId, ops.length === 1 ? ops[0] : ops)
  }
  container.pendingOps.length = 0
}

function requireContainerWindowId(container: HostContainer): string {
  if (container.windowId === null) {
    throw new Error('single-window React root is missing a window id')
  }
  return container.windowId
}

export function dispatchHostCallbacks(
  container: HostContainer,
  invocations: readonly CallbackInvocation[],
): number {
  let dispatched = 0
  let needsFlush = false
  for (const invocation of invocations) {
    const previousControlledText = controlledTextValueForInvocation(container, invocation)
    if (!container.eventDispatcher.dispatch(invocation)) continue
    dispatched += 1
    if (resyncControlledTextAfterChange(container, invocation, previousControlledText)) {
      needsFlush = true
    }
  }
  if (needsFlush) {
    flushStaticTree(container)
  }
  return dispatched
}

export interface WindowLifecycleEvent {
  readonly windowId: string
  readonly type: 'closed' | 'minimized' | 'maximized' | 'restored'
  readonly state: string | null
}

/** Route window lifecycle events drained from the host to their Window's callbacks. */
export function dispatchWindowEvents(
  container: HostContainer,
  events: readonly WindowLifecycleEvent[],
): number {
  let dispatched = 0
  for (const event of events) {
    const instance = findWindowInstance(container, event.windowId)
    if (!instance) continue
    const handler = instance.windowLifecycle[WINDOW_EVENT_TYPE_TO_KEY[event.type]]
    if (typeof handler === 'function') {
      ;(handler as (event: WindowLifecycleEvent) => void)(event)
      dispatched += 1
    }
  }
  return dispatched
}

function findWindowInstance(container: HostContainer, windowId: string): HostInstance | null {
  for (const child of container.rootChildren) {
    if (isWindowInstance(child) && child.windowId === windowId) return child
  }
  return null
}

export function prepareHostUpdate(
  oldProps: Readonly<Record<string, unknown>>,
  newProps: Readonly<Record<string, unknown>>,
): HostUpdatePayload | null {
  const oldHostProps = sanitizeProps(oldProps)
  const newHostProps = sanitizeProps(newProps)
  const oldControlledText = oldProps[CONTROLLED_TEXT_PROP] === true
  const newControlledText = newProps[CONTROLLED_TEXT_PROP] === true
  const setProps: HostPropUpdate[] = []

  for (const [name, value] of Object.entries(newHostProps)) {
    if (!componentValueEqual(oldHostProps[name], value)) {
      setProps.push({ name, value })
    }
  }
  const clearProps = Object.keys(oldHostProps).filter(
    (name) => !Object.prototype.hasOwnProperty.call(newHostProps, name),
  )

  const oldEvents = eventProps(oldProps)
  const newEvents = eventProps(newProps)
  const bindEvents: HostEventUpdate[] = []
  const clearEvents: string[] = []
  const updateEvents: HostEventUpdate[] = []

  for (const [event, handler] of newEvents) {
    if (!oldEvents.has(event)) {
      bindEvents.push({ event, handler })
    } else if (!Object.is(oldEvents.get(event), handler)) {
      updateEvents.push({ event, handler })
    }
  }
  for (const event of oldEvents.keys()) {
    if (!newEvents.has(event)) {
      clearEvents.push(event)
    }
  }

  if (
    setProps.length === 0
    && clearProps.length === 0
    && bindEvents.length === 0
    && clearEvents.length === 0
    && updateEvents.length === 0
    && oldControlledText === newControlledText
  ) {
    return null
  }

  return {
    props: newHostProps,
    controlledText: newControlledText,
    setProps,
    clearProps,
    bindEvents,
    clearEvents,
    updateEvents,
  }
}

function resyncControlledTextAfterChange(
  container: HostContainer,
  invocation: CallbackInvocation,
  previousText: string | null,
): boolean {
  if (previousText === null || invocation.event !== 'change' || invocation.targetId === null) {
    return false
  }
  const instance = findHostInstance(container, invocation.targetId)
  if (!instance || !instance.controlledText || !isControlledTextType(instance.type)) return false
  const text = instance.props.text
  if (typeof text !== 'string' || invocation.payload === text || text !== previousText) return false
  enqueueTreeOpForMountedInstance(instance, {
    op: 'set_prop',
    id: instance.id,
    name: 'text',
    value: text,
  })
  return true
}

function controlledTextValueForInvocation(
  container: HostContainer,
  invocation: CallbackInvocation,
): string | null {
  if (invocation.event !== 'change' || invocation.targetId === null) return null
  const instance = findHostInstance(container, invocation.targetId)
  if (!instance || !instance.controlledText || !isControlledTextType(instance.type)) return null
  const text = instance.props.text
  return typeof text === 'string' ? text : null
}

function findHostInstance(container: HostContainer, id: string): HostInstance | null {
  for (const child of container.rootChildren) {
    const found = findHostInstanceInSubtree(child, id)
    if (found) return found
  }
  return null
}

function findHostInstanceInSubtree(instance: HostInstance, id: string): HostInstance | null {
  if (instance.id === id) return instance
  for (const child of instance.children) {
    const found = findHostInstanceInSubtree(child, id)
    if (found) return found
  }
  return null
}

export function commitHostUpdate(instance: HostInstance, payload: HostUpdatePayload): void {
  instance.controlledText = payload.controlledText
  if (isWindowInstance(instance)) {
    commitWindowUpdate(instance, payload)
    return
  }
  if (isStatusBarInstance(instance)) {
    instance.props = payload.props
    syncStatusBarInstance(instance)
    return
  }
  if (isMenuTreeInstance(instance)) {
    instance.props = payload.props
    commitEventUpdates(instance, payload, false)
    markMenuBarForSync(instance)
    return
  }

  instance.props = payload.props

  for (const { name, value } of payload.setProps) {
    enqueueTreeOpForMountedInstance(instance, {
      op: 'set_prop',
      id: instance.id,
      name,
      value,
    })
  }

  for (const name of payload.clearProps) {
    enqueueTreeOpForMountedInstance(instance, {
      op: 'clear_prop',
      id: instance.id,
      name,
    })
  }

  commitEventUpdates(instance, payload, true)
}

function commitEventUpdates(
  instance: HostInstance,
  payload: HostUpdatePayload,
  queueTreeOps: boolean,
): void {
  for (const event of payload.clearEvents) {
    const binding = instance.events[event]
    if (queueTreeOps) {
      enqueueTreeOpForMountedInstance(instance, { op: 'clear_event', id: instance.id, event })
    }
    if (binding) {
      unregisterEventBinding(instance, binding)
      delete instance.events[event]
    }
  }

  for (const { event, handler } of payload.bindEvents) {
    const callbackId = registerEventBindingForInstance(instance, handler)
    instance.events[event] = { callbackId, handler }
    if (queueTreeOps) {
      enqueueTreeOpForMountedInstance(instance, {
        op: 'bind_event',
        id: instance.id,
        event,
        callback: callbackId,
      })
    }
  }

  for (const { event, handler } of payload.updateEvents) {
    const binding = instance.events[event]
    if (binding) {
      binding.handler = handler
      updateEventBinding(instance, binding, handler)
    }
  }
}

export function sanitizeProps(props: Readonly<Record<string, unknown>>): HostProps {
  const sanitized: Record<string, ComponentValue> = {}
  for (const [name, value] of Object.entries(props)) {
    if (shouldSkipProp(name, value)) continue
    const converted = toComponentValue(value)
    if (converted !== undefined) {
      sanitized[name] = converted
    }
  }
  return sanitized
}

export function normalizeHostType(type: string): string {
  const mapped = HOST_TYPE_NAMES[type]
  if (mapped) return mapped
  if (type.length === 0) return type
  return type[0].toUpperCase() + type.slice(1)
}

function createEventBindings(
  container: HostContainer,
  props: Readonly<Record<string, unknown>>,
  isWindow = false,
): HostEventBindings {
  const bindings: HostEventBindings = {}
  for (const [event, handler] of eventProps(props)) {
    // Window lifecycle callbacks are not component events — they have no runtime
    // component id, so allocating a callback would leak an id that never fires.
    if (isWindow && event in WINDOW_LIFECYCLE_EVENTS) continue
    bindings[event] = { callbackId: container.eventDispatcher.register(handler), handler }
  }
  return bindings
}

function extractWindowLifecycle(props: Readonly<Record<string, unknown>>): WindowLifecycle {
  const lifecycle: WindowLifecycle = {}
  for (const [event, handler] of eventProps(props)) {
    const key = WINDOW_LIFECYCLE_EVENTS[event]
    if (key) lifecycle[key] = handler
  }
  return lifecycle
}

/** Apply a prepared update to a Window instance's lifecycle callbacks (no runtime callbacks). */
function applyWindowLifecycleUpdates(instance: HostInstance, payload: HostUpdatePayload): void {
  for (const { event, handler } of [...payload.bindEvents, ...payload.updateEvents]) {
    const key = WINDOW_LIFECYCLE_EVENTS[event]
    if (key) instance.windowLifecycle[key] = handler
  }
  for (const event of payload.clearEvents) {
    const key = WINDOW_LIFECYCLE_EVENTS[event]
    if (key) delete instance.windowLifecycle[key]
  }
}

function eventsToSpec(bindings: HostEventBindings): Readonly<Record<string, string>> {
  const events: Record<string, string> = {}
  for (const [event, binding] of Object.entries(bindings)) {
    events[event] = binding.callbackId
  }
  return events
}

function eventProps(props: Readonly<Record<string, unknown>>): Map<string, unknown> {
  const events = new Map<string, unknown>()
  for (const [name, value] of Object.entries(props)) {
    if (typeof value !== 'function') continue
    const event = eventNameFromProp(name)
    if (event) {
      events.set(event, value)
    }
  }
  return events
}

function eventNameFromProp(name: string): string | null {
  if (!/^on[A-Z]/.test(name)) return null
  const raw = name.slice(2)
  return raw[0].toLowerCase() + raw.slice(1)
}

function registerEventBindingForInstance(instance: HostInstance, handler: unknown): string {
  const container = containerForInstance(instance)
  if (!container) {
    throw new Error(`Cannot allocate callback for detached node ${instance.id}`)
  }
  return container.eventDispatcher.register(handler)
}

function updateEventBinding(
  instance: HostInstance,
  binding: HostEventBinding,
  handler: unknown,
): void {
  const container = containerForInstance(instance)
  if (!container) return
  container.eventDispatcher.update(binding.callbackId, handler)
}

function unregisterEventBinding(instance: HostInstance, binding: HostEventBinding): void {
  const container = containerForInstance(instance)
  if (!container) return
  container.eventDispatcher.unregister(binding.callbackId)
}

function enqueueChildInsert(
  parent: HostInstance,
  child: HostInstance,
  beforeChild: HostInstance | null,
): void {
  enqueueTreeOpForInstance(parent, {
    op: 'insert_before',
    parent_id: parent.id,
    anchor_id: beforeChild?.id ?? null,
    child: childToComponentSpec(child),
  })
}

function enqueueClearEventsForSubtree(anchor: HostInstance, instance: HostInstance): void {
  for (const event of Object.keys(instance.events)) {
    enqueueTreeOpForInstance(anchor, { op: 'clear_event', id: instance.id, event })
  }
  for (const child of instance.children) {
    enqueueClearEventsForSubtree(anchor, child)
  }
}

function releaseEventBindingsForSubtree(instance: HostInstance): void {
  for (const event of Object.keys(instance.events)) {
    const binding = instance.events[event]
    if (binding) {
      unregisterEventBinding(instance, binding)
      delete instance.events[event]
    }
  }
  for (const child of instance.children) {
    releaseEventBindingsForSubtree(child)
  }
}

function enqueueTreeOpForMountedInstance(instance: HostInstance, op: TreeOp): void {
  if (instance.windowId !== null) {
    enqueueTreeOpForInstance(instance, op)
  } else {
    markContainerForFlush(instance)
  }
}

function enqueueTreeOpForInstance(instance: HostInstance, op: TreeOp): void {
  const container = containerForInstance(instance)
  if (!container) {
    throw new Error(`Cannot enqueue TreeOp for detached node ${instance.id}`)
  }
  const windowId = instance.windowId
  if (windowId === null) {
    markNearestTreeRootForFlush(instance)
    return
  }
  if (!container.needsTreeFlush) {
    container.pendingOps.push({ windowId, op })
  }
}

function containerForInstance(instance: HostInstance): HostContainer | null {
  let cursor: HostContainer | HostInstance | null = instance
  while (cursor !== null) {
    if ('rootChildren' in cursor) return cursor
    cursor = cursor.parent
  }
  return null
}

function emptyRootSpec(container: HostContainer): ComponentSpec {
  return { type: 'Spacer', id: `${container.idPrefix}-empty-root` }
}

function componentValueEqual(left: ComponentValue | undefined, right: ComponentValue): boolean {
  if (left === undefined) return false
  if (Object.is(left, right)) return true
  if (left instanceof Uint8Array || right instanceof Uint8Array) {
    return left instanceof Uint8Array && right instanceof Uint8Array && bytesEqual(left, right)
  }
  if (Array.isArray(left) || Array.isArray(right)) {
    if (!Array.isArray(left) || !Array.isArray(right) || left.length !== right.length) return false
    return left.every((value, index) => componentValueEqual(value, right[index]))
  }
  if (isComponentValueMap(left) && isComponentValueMap(right)) {
    const leftMap = left as Record<string, ComponentValue>
    const rightMap = right as Record<string, ComponentValue>
    const leftKeys = Object.keys(leftMap)
    const rightKeys = Object.keys(rightMap)
    if (leftKeys.length !== rightKeys.length) return false
    return leftKeys.every((key) => componentValueEqual(leftMap[key], rightMap[key]))
  }
  return false
}

function bytesEqual(left: Uint8Array, right: Uint8Array): boolean {
  if (left.length !== right.length) return false
  return left.every((value, index) => value === right[index])
}

function isComponentValueMap(value: ComponentValue | undefined): value is Record<string, ComponentValue> {
  return typeof value === 'object'
    && value !== null
    && !(value instanceof Uint8Array)
    && !Array.isArray(value)
}

function attachChild(
  parent: HostInstance,
  child: HostInstance,
  beforeChild: HostInstance | null,
): void {
  validateChildForParent(parent, child)
  detachFromParent(child)
  const index = beforeChild === null ? parent.children.length : parent.children.indexOf(beforeChild)
  if (index < 0) {
    throw new Error(`Cannot insert before unknown child ${beforeChild?.id ?? '<null>'}`)
  }
  parent.children.splice(index, 0, child)
  child.parent = parent
  setSubtreeWindowId(child, parent.windowId)
}

function attachRootChild(
  container: HostContainer,
  child: HostInstance,
  beforeChild: HostInstance | null,
): void {
  validateRootChild(container, child)
  detachFromParent(child)
  const index = beforeChild === null
    ? container.rootChildren.length
    : container.rootChildren.indexOf(beforeChild)
  if (index < 0) {
    throw new Error(`Cannot insert before unknown root child ${beforeChild?.id ?? '<null>'}`)
  }
  container.rootChildren.splice(index, 0, child)
  child.parent = container
  setSubtreeWindowId(child, container.mode === 'window' ? container.windowId : child.windowId)
}

function detachFromParent(child: HostInstance): void {
  const parent = child.parent
  if (parent === null) return
  const siblings = 'rootChildren' in parent ? parent.rootChildren : parent.children
  const index = siblings.indexOf(child)
  if (index >= 0) {
    siblings.splice(index, 1)
  }
}

function setSubtreeWindowId(instance: HostInstance, windowId: string | null): void {
  instance.windowId = windowId
  for (const child of instance.children) {
    setSubtreeWindowId(child, windowId)
  }
}

function markContainerForFlush(instance: HostInstance): void {
  let cursor: HostContainer | HostInstance | null = instance
  while (cursor !== null) {
    if (!('rootChildren' in cursor) && isWindowInstance(cursor)) {
      cursor.needsTreeFlush = true
      return
    }
    if ('rootChildren' in cursor) {
      cursor.needsTreeFlush = true
      return
    }
    cursor = cursor.parent
  }
}

function markNearestTreeRootForFlush(instance: HostInstance): void {
  let cursor: HostContainer | HostInstance | null = instance
  while (cursor !== null) {
    if (!('rootChildren' in cursor) && isWindowInstance(cursor)) {
      markWindowForTreeFlush(cursor)
      return
    }
    if ('rootChildren' in cursor) {
      cursor.needsTreeFlush = true
      return
    }
    cursor = cursor.parent
  }
}

function markWindowForTreeFlush(instance: HostInstance): void {
  if (!isWindowInstance(instance)) {
    throw new Error(`Cannot mark non-window node ${instance.type} for window root flush`)
  }
  instance.needsTreeFlush = true
}

function mountDesktopChild(container: HostContainer, child: HostInstance): void {
  if (isWindowInstance(child)) {
    if (child.windowId === null) {
      const host = requireDesktopHost(container)
      const { title, rect } = windowOptions(child)
      const tree = windowRootSpec(child)
      const windowId = host.addDynamicWindow(title, rect, tree)
      setSubtreeWindowId(child, windowId)
      child.lastTree = tree
      child.needsTreeFlush = false
    }
    return
  }

  if (isMenuBarInstance(child)) {
    syncMenuBarInstance(child)
    return
  }

  if (isStatusBarInstance(child)) {
    syncStatusBarInstance(child)
  }
}

function unmountDesktopChild(container: HostContainer, child: HostInstance): void {
  const host = requireDesktopHost(container)
  if (isWindowInstance(child)) {
    const windowId = child.windowId
    if (windowId !== null) {
      // The window may already be gone if the user closed it from the TUI (the
      // onClose callback then unmounts this <Window>). Its handle is released, so
      // closeWindow would throw unknown-handle — the close already happened.
      try {
        host.closeWindow(windowId)
      } catch {
        // Window already closed by the TUI; nothing more to do.
      }
      container.pendingOps.splice(
        0,
        container.pendingOps.length,
        ...container.pendingOps.filter((pending) => pending.windowId !== windowId),
      )
    }
    child.lastTree = null
    child.needsTreeFlush = false
    return
  }

  if (isMenuBarInstance(child)) {
    host.setMenuBar({ menus: [] })
    child.needsDesktopSync = false
    return
  }

  if (isStatusBarInstance(child)) {
    host.setStatusBar(null, null)
    child.needsDesktopSync = false
  }
}

function commitWindowUpdate(instance: HostInstance, payload: HostUpdatePayload): void {
  const oldOptions = instance.windowId === null ? null : windowOptions(instance)
  instance.props = payload.props
  applyWindowLifecycleUpdates(instance, payload)

  const windowId = instance.windowId
  if (windowId === null) return

  const host = requireDesktopHostForInstance(instance)
  const nextOptions = windowOptions(instance)
  if (oldOptions === null || oldOptions.title !== nextOptions.title) {
    host.setTitle(windowId, nextOptions.title)
  }
  if (oldOptions === null || oldOptions.rect.x !== nextOptions.rect.x || oldOptions.rect.y !== nextOptions.rect.y) {
    host.moveWindow(windowId, nextOptions.rect.x, nextOptions.rect.y)
  }
  if (
    oldOptions === null
    || oldOptions.rect.width !== nextOptions.rect.width
    || oldOptions.rect.height !== nextOptions.rect.height
  ) {
    host.resizeWindow(windowId, nextOptions.rect.width, nextOptions.rect.height)
  }
}

function syncStatusBarInstance(instance: HostInstance): void {
  const host = requireDesktopHostForInstance(instance)
  host.setStatusBar(stringProp(instance, 'left'), stringProp(instance, 'right'))
  instance.needsDesktopSync = false
}

function syncMenuBarInstance(instance: HostInstance): void {
  const host = requireDesktopHostForInstance(instance)
  host.setMenuBar(menuBarSpec(instance))
  instance.needsDesktopSync = false
}

function markMenuBarForSync(instance: HostInstance): void {
  const menuBar = findMenuBarAncestor(instance)
  if (menuBar) {
    menuBar.needsDesktopSync = true
  }
}

function findMenuBarAncestor(instance: HostInstance): HostInstance | null {
  let cursor: HostContainer | HostInstance | null = instance
  while (cursor !== null) {
    if (!('rootChildren' in cursor) && isMenuBarInstance(cursor)) return cursor
    if ('rootChildren' in cursor) return null
    cursor = cursor.parent
  }
  return null
}

function windowRootSpec(windowInstance: HostInstance): ComponentSpec {
  if (windowInstance.children.length > 1) {
    throw new Error('<Window> requires at most one runtime root child')
  }
  if (windowInstance.children.length === 0) {
    return { type: 'Spacer', id: `${windowInstance.id}-empty-root` }
  }
  return toComponentSpec(windowInstance.children[0])
}

function windowOptions(instance: HostInstance): { title: string; rect: Rect } {
  return {
    title: stringProp(instance, 'title') ?? 'atto-ui React',
    rect: rectProp(instance, 'rect'),
  }
}

function menuBarSpec(instance: HostInstance): MenuBarSpec {
  if (!isMenuBarInstance(instance)) {
    throw new Error(`Expected MenuBar node, got ${instance.type}`)
  }
  return {
    menus: instance.children.map((child) => {
      if (!isMenuInstance(child)) {
        throw new Error('<MenuBar> children must be <Menu> nodes')
      }
      return {
        id: stringProp(child, 'id') ?? child.id,
        title: requiredStringProp(child, 'title'),
        items: child.children.map(menuItemSpec),
      }
    }),
  }
}

function menuItemSpec(instance: HostInstance): MenuItemSpec {
  if (!isMenuItemInstance(instance)) {
    throw new Error('<Menu> and <MenuItem> children must be <MenuItem> nodes')
  }
  const click = instance.events.click?.callbackId
  return {
    id: stringProp(instance, 'id') ?? instance.id,
    label: requiredStringProp(instance, 'label'),
    shortcut: stringProp(instance, 'shortcut'),
    enabled: boolProp(instance, 'enabled') ?? true,
    callback: click ?? null,
    items: instance.children.map(menuItemSpec),
  }
}

function requiredStringProp(instance: HostInstance, name: string): string {
  const value = stringProp(instance, name)
  if (value === null) {
    throw new Error(`${instance.type} requires string prop ${name}`)
  }
  return value
}

function stringProp(instance: HostInstance, name: string): string | null {
  const value = instance.props[name]
  return typeof value === 'string' ? value : null
}

function boolProp(instance: HostInstance, name: string): boolean | null {
  const value = instance.props[name]
  return typeof value === 'boolean' ? value : null
}

function rectProp(instance: HostInstance, name: string): Rect {
  const value = instance.props[name]
  if (Array.isArray(value)) {
    const [x, y, width, height] = value
    if (
      typeof x === 'number'
      && typeof y === 'number'
      && typeof width === 'number'
      && typeof height === 'number'
    ) {
      return { x, y, width, height }
    }
  }
  if (isComponentValueMap(value)) {
    const rect = value as Record<string, ComponentValue>
    const { x, y, width, height } = rect
    if (
      typeof x === 'number'
      && typeof y === 'number'
      && typeof width === 'number'
      && typeof height === 'number'
    ) {
      return { x, y, width, height }
    }
  }
  throw new Error(`<Window> requires rect prop as { x, y, width, height } or [x, y, width, height]`)
}

function requireDesktopHost(container: HostContainer): DesktopRenderHost {
  const host = container.host as Partial<DesktopRenderHost>
  if (
    typeof host.addDynamicWindow !== 'function'
    || typeof host.closeWindow !== 'function'
    || typeof host.moveWindow !== 'function'
    || typeof host.resizeWindow !== 'function'
    || typeof host.setTitle !== 'function'
    || typeof host.setMenuBar !== 'function'
    || typeof host.setStatusBar !== 'function'
  ) {
    throw new Error('DesktopContainer requires AppHost window and desktop chrome methods')
  }
  return host as DesktopRenderHost
}

function requireDesktopHostForInstance(instance: HostInstance): DesktopRenderHost {
  const container = containerForInstance(instance)
  if (!container) {
    throw new Error(`Cannot find DesktopContainer for ${instance.id}`)
  }
  return requireDesktopHost(container)
}

function validateRootChild(container: HostContainer, child: HostInstance): void {
  if (container.mode === 'desktop') {
    if (!isDesktopRootChild(child)) {
      throw new Error('DesktopContainer direct children must be <Window>, <MenuBar>, or <StatusBar>')
    }
    return
  }

  if (isVirtualDesktopInstance(child)) {
    throw new Error(`${child.type} can only be mounted under a DesktopContainer`)
  }
}

function validateChildForParent(parent: HostInstance, child: HostInstance): void {
  if (isWindowInstance(parent)) {
    if (isVirtualDesktopInstance(child)) {
      throw new Error('<Window> children must be regular runtime components')
    }
    return
  }

  if (isMenuBarInstance(parent)) {
    if (!isMenuInstance(child)) {
      throw new Error('<MenuBar> children must be <Menu> nodes')
    }
    return
  }

  if (isMenuInstance(parent) || isMenuItemInstance(parent)) {
    if (!isMenuItemInstance(child)) {
      throw new Error('<Menu> and <MenuItem> children must be <MenuItem> nodes')
    }
    return
  }

  if (isStatusBarInstance(parent) || isVirtualDesktopInstance(child)) {
    throw new Error(`${child.type} cannot be mounted under ${parent.type}`)
  }
}

function isDesktopRootChild(instance: HostInstance): boolean {
  return isWindowInstance(instance) || isMenuBarInstance(instance) || isStatusBarInstance(instance)
}

function isVirtualDesktopInstance(instance: HostInstance): boolean {
  return isDesktopRootChild(instance) || isMenuInstance(instance) || isMenuItemInstance(instance)
}

function isWindowInstance(instance: HostInstance): boolean {
  return instance.type === 'Window'
}

function isMenuBarInstance(instance: HostInstance): boolean {
  return instance.type === 'MenuBar'
}

function isMenuInstance(instance: HostInstance): boolean {
  return instance.type === 'Menu'
}

function isMenuItemInstance(instance: HostInstance): boolean {
  return instance.type === 'MenuItem'
}

function isMenuTreeInstance(instance: HostInstance): boolean {
  return isMenuBarInstance(instance) || isMenuInstance(instance) || isMenuItemInstance(instance)
}

function isStatusBarInstance(instance: HostInstance): boolean {
  return instance.type === 'StatusBar'
}

function shouldSkipProp(name: string, value: unknown): boolean {
  if (name === 'children' || name === 'key' || name === 'ref') return true
  // `layout` is lifted to the ComponentSpecChild wrapper, not a runtime prop.
  if (name === 'layout') return true
  if (name.startsWith('__atto')) return true
  if (value === undefined || typeof value === 'function' || typeof value === 'symbol') return true
  return /^on[A-Z]/.test(name)
}

function isControlledTextType(type: string): boolean {
  return type === 'TextBox' || type === 'TextArea'
}

function isControlledTextProps(type: string, props: Readonly<Record<string, unknown>>): boolean {
  return isControlledTextType(type) && props[CONTROLLED_TEXT_PROP] === true
}

function toComponentValue(value: unknown): ComponentValue | undefined {
  if (value === undefined || typeof value === 'function' || typeof value === 'symbol') {
    return undefined
  }
  if (value === null || typeof value === 'boolean' || typeof value === 'number' || typeof value === 'string') {
    return value
  }
  if (value instanceof Uint8Array) {
    return value
  }
  if (Array.isArray(value)) {
    return value.map((item) => toComponentValue(item) ?? null)
  }
  if (typeof value === 'object') {
    const converted: Record<string, ComponentValue> = {}
    for (const [key, item] of Object.entries(value as Record<string, unknown>)) {
      const nested = toComponentValue(item)
      if (nested !== undefined) {
        converted[key] = nested
      }
    }
    return converted
  }
  return undefined
}

const HOST_TYPE_NAMES: Readonly<Record<string, string>> = {
  border: 'Border',
  button: 'Button',
  checkbox: 'Checkbox',
  commandPalette: 'CommandPalette',
  commandpalette: 'CommandPalette',
  divider: 'Divider',
  disclosure: 'Disclosure',
  editor: 'Editor',
  fileTree: 'FileTree',
  filetree: 'FileTree',
  grid: 'Grid',
  hstack: 'HStack',
  label: 'Label',
  listBox: 'ListBox',
  listbox: 'ListBox',
  markdownViewer: 'MarkdownViewer',
  markdownviewer: 'MarkdownViewer',
  menu: 'Menu',
  menuBar: 'MenuBar',
  menubar: 'MenuBar',
  menuItem: 'MenuItem',
  menuitem: 'MenuItem',
  progressBar: 'ProgressBar',
  progressbar: 'ProgressBar',
  radioGroup: 'RadioGroup',
  radiogroup: 'RadioGroup',
  richText: 'RichText',
  richtext: 'RichText',
  slider: 'Slider',
  spacer: 'Spacer',
  spinner: 'Spinner',
  splitter: 'Splitter',
  styledLabel: 'StyledLabel',
  styledlabel: 'StyledLabel',
  tableView: 'TableView',
  tableview: 'TableView',
  tabView: 'TabView',
  tabview: 'TabView',
  text: 'Text',
  textArea: 'TextArea',
  textarea: 'TextArea',
  textBox: 'TextBox',
  textbox: 'TextBox',
  textSpan: 'TextSpan',
  textspan: 'TextSpan',
  typeAhead: 'TypeAhead',
  typeahead: 'TypeAhead',
  visibility: 'Visibility',
  vstack: 'VStack',
  window: 'Window',
}
