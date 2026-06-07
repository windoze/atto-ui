import type {
  AppHost,
  ComponentSpec,
  ComponentSpecChild,
  ComponentValue,
  TreeOp,
} from '@atto-ui/core'

export type RenderHost = Pick<AppHost, 'applyTreeOps'>

export interface HostContainerOptions {
  readonly idPrefix?: string
}

export interface HostContainer {
  readonly host: RenderHost
  readonly windowId: string
  readonly idPrefix: string
  readonly rootChildren: HostInstance[]
  nextId: number
  needsTreeFlush: boolean
  lastTree: ComponentSpec | null
}

export interface HostInstance {
  readonly id: string
  readonly type: string
  props: HostProps
  readonly children: HostInstance[]
  windowId: string | null
  parent: HostContainer | HostInstance | null
}

export type HostProps = Readonly<Record<string, ComponentValue>>

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
    children: [],
    windowId: null,
    parent: null,
  }
}

/** Represent raw React text as a plain TextSpan until richer text support lands. */
export function createHostTextInstance(container: HostContainer, text: string): HostInstance {
  return {
    id: `${container.idPrefix}-${++container.nextId}`,
    type: 'TextSpan',
    props: text ? { text } : {},
    children: [],
    windowId: null,
    parent: null,
  }
}

export function appendInitialChild(parent: HostInstance, child: HostInstance): void {
  attachChild(parent, child, null)
}

export function appendChild(parent: HostInstance, child: HostInstance): void {
  attachChild(parent, child, null)
  markContainerForFlush(parent)
}

export function insertBefore(
  parent: HostInstance,
  child: HostInstance,
  beforeChild: HostInstance,
): void {
  attachChild(parent, child, beforeChild)
  markContainerForFlush(parent)
}

export function removeChild(parent: HostInstance, child: HostInstance): void {
  detachFromParent(child)
  if (child.parent === parent) {
    child.parent = null
  }
  markContainerForFlush(parent)
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
  detachFromParent(child)
  if (child.parent === container) {
    child.parent = null
  }
  container.needsTreeFlush = true
}

export function clearContainer(container: HostContainer): boolean {
  for (const child of container.rootChildren) {
    child.parent = null
    child.windowId = null
  }
  container.rootChildren.length = 0
  container.needsTreeFlush = true
  return false
}

export function updateTextInstance(textInstance: HostInstance, text: string): void {
  textInstance.props = text ? { text } : {}
  markContainerForFlush(textInstance)
}

export function toComponentSpec(instance: HostInstance): ComponentSpec {
  const spec: {
    type: string
    id: string
    props?: HostProps
    children?: readonly ComponentSpecChild[]
  } = {
    type: instance.type,
    id: instance.id,
  }

  if (Object.keys(instance.props).length > 0) {
    spec.props = instance.props
  }
  if (instance.children.length > 0) {
    spec.children = instance.children.map(toComponentSpec)
  }

  return spec
}

/** Flush the current static React subtree into the target atto-ui window. */
export function flushStaticTree(container: HostContainer): void {
  if (!container.needsTreeFlush) return
  if (container.rootChildren.length === 0) return
  if (container.rootChildren.length !== 1) {
    throw new Error('atto-ui React root currently requires exactly one host child')
  }

  const tree = toComponentSpec(container.rootChildren[0])
  const op: TreeOp = { op: 'set_tree', tree }
  container.host.applyTreeOps(container.windowId, op)
  container.lastTree = tree
  container.needsTreeFlush = false
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
