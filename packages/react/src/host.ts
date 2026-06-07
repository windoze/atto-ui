import type {
  AppHost,
  CallbackInvocation,
  ComponentSpec,
  ComponentSpecChild,
  ComponentValue,
  TreeOp,
} from '@atto-ui/core'

import { CallbackEventDispatcher } from './events'

export type RenderHost = Pick<AppHost, 'applyTreeOps' | 'allocCallback'> & {
  releaseCallback?(callbackId: string): boolean
}

export interface HostContainerOptions {
  readonly idPrefix?: string
}

export interface HostContainer {
  readonly host: RenderHost
  readonly windowId: string
  readonly idPrefix: string
  readonly rootChildren: HostInstance[]
  readonly pendingOps: TreeOp[]
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
}

export type HostProps = Readonly<Record<string, ComponentValue>>
export type HostEventBindings = Record<string, HostEventBinding>

export interface HostEventBinding {
  callbackId: string
  handler: unknown
}

export interface HostUpdatePayload {
  readonly props: HostProps
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

/** Create the single-window container that the React reconciler mutates. */
export function createHostContainer(
  host: RenderHost,
  windowId: string,
  options: HostContainerOptions = {},
): HostContainer {
  const idPrefix = options.idPrefix ?? `atto-react-${nextContainerId++}`
  return {
    host,
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

/** Allocate a host component instance and assign its runtime ComponentSpec id. */
export function createHostInstance(
  container: HostContainer,
  type: string,
  props: Readonly<Record<string, unknown>>,
): HostInstance {
  return {
    id: `${container.idPrefix}-${++container.nextId}`,
    type: normalizeHostType(type),
    props: sanitizeProps(props),
    events: createEventBindings(container, props),
    children: [],
    windowId: null,
    parent: null,
  }
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
  }
}

export function appendInitialChild(parent: HostInstance, child: HostInstance): void {
  attachChild(parent, child, null)
}

export function appendChild(parent: HostInstance, child: HostInstance): void {
  const shouldQueue = parent.windowId !== null
  attachChild(parent, child, null)
  if (shouldQueue) {
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
  const shouldQueue = parent.windowId !== null
  attachChild(parent, child, beforeChild)
  if (shouldQueue) {
    enqueueChildInsert(parent, child, beforeChild)
  } else {
    markContainerForFlush(parent)
  }
}

export function removeChild(parent: HostInstance, child: HostInstance): void {
  const shouldQueue = parent.windowId !== null && child.windowId !== null
  if (shouldQueue) {
    enqueueClearEventsForSubtree(parent, child)
    enqueueTreeOpForInstance(parent, { op: 'remove', id: child.id })
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
  attachRootChild(container, child, null)
  container.needsTreeFlush = true
}

export function insertInContainerBefore(
  container: HostContainer,
  child: HostInstance,
  beforeChild: HostInstance,
): void {
  attachRootChild(container, child, beforeChild)
  container.needsTreeFlush = true
}

export function removeChildFromContainer(container: HostContainer, child: HostInstance): void {
  releaseEventBindingsForSubtree(child)
  detachFromParent(child)
  if (child.parent === container) {
    child.parent = null
  }
  setSubtreeWindowId(child, null)
  container.needsTreeFlush = true
}

export function clearContainer(container: HostContainer): boolean {
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
    spec.children = instance.children.map(toComponentSpec)
  }

  return spec
}

/** Flush root replacement or incremental TreeOp mutations into the target atto-ui window. */
export function flushStaticTree(container: HostContainer): void {
  if (container.needsTreeFlush) {
    if (container.rootChildren.length > 1) {
      throw new Error('atto-ui React root currently requires at most one host child')
    }

    const tree = container.rootChildren.length === 0
      ? emptyRootSpec(container)
      : toComponentSpec(container.rootChildren[0])
    const op: TreeOp = { op: 'set_tree', tree }
    container.host.applyTreeOps(container.windowId, op)
    container.lastTree = tree
    container.pendingOps.length = 0
    container.needsTreeFlush = false
    return
  }

  if (container.pendingOps.length === 0) return
  const ops = container.pendingOps.length === 1
    ? container.pendingOps[0]
    : container.pendingOps.slice()
  container.host.applyTreeOps(container.windowId, ops)
  container.pendingOps.length = 0
  container.lastTree = container.rootChildren.length === 0
    ? emptyRootSpec(container)
    : toComponentSpec(container.rootChildren[0])
}

export function dispatchHostCallbacks(
  container: HostContainer,
  invocations: readonly CallbackInvocation[],
): number {
  return container.eventDispatcher.dispatchAll(invocations)
}

export function prepareHostUpdate(
  oldProps: Readonly<Record<string, unknown>>,
  newProps: Readonly<Record<string, unknown>>,
): HostUpdatePayload | null {
  const oldHostProps = sanitizeProps(oldProps)
  const newHostProps = sanitizeProps(newProps)
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
  ) {
    return null
  }

  return {
    props: newHostProps,
    setProps,
    clearProps,
    bindEvents,
    clearEvents,
    updateEvents,
  }
}

export function commitHostUpdate(instance: HostInstance, payload: HostUpdatePayload): void {
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

  for (const event of payload.clearEvents) {
    const binding = instance.events[event]
    enqueueTreeOpForMountedInstance(instance, { op: 'clear_event', id: instance.id, event })
    if (binding) {
      unregisterEventBinding(instance, binding)
      delete instance.events[event]
    }
  }

  for (const { event, handler } of payload.bindEvents) {
    const callbackId = registerEventBindingForInstance(instance, handler)
    instance.events[event] = { callbackId, handler }
    enqueueTreeOpForMountedInstance(instance, {
      op: 'bind_event',
      id: instance.id,
      event,
      callback: callbackId,
    })
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
): HostEventBindings {
  const bindings: HostEventBindings = {}
  for (const [event, handler] of eventProps(props)) {
    bindings[event] = { callbackId: container.eventDispatcher.register(handler), handler }
  }
  return bindings
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
    child: toComponentSpec(child),
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
  if (!container.needsTreeFlush) {
    container.pendingOps.push(op)
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
  detachFromParent(child)
  const index = beforeChild === null
    ? container.rootChildren.length
    : container.rootChildren.indexOf(beforeChild)
  if (index < 0) {
    throw new Error(`Cannot insert before unknown root child ${beforeChild?.id ?? '<null>'}`)
  }
  container.rootChildren.splice(index, 0, child)
  child.parent = container
  setSubtreeWindowId(child, container.windowId)
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
    if ('rootChildren' in cursor) {
      cursor.needsTreeFlush = true
      return
    }
    cursor = cursor.parent
  }
}

function shouldSkipProp(name: string, value: unknown): boolean {
  if (name === 'children' || name === 'key' || name === 'ref') return true
  if (value === undefined || typeof value === 'function' || typeof value === 'symbol') return true
  return /^on[A-Z]/.test(name)
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
  grid: 'Grid',
  hstack: 'HStack',
  label: 'Label',
  listBox: 'ListBox',
  listbox: 'ListBox',
  markdownViewer: 'MarkdownViewer',
  markdownviewer: 'MarkdownViewer',
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
}
