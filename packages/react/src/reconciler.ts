import type { ReactNode } from 'react'
import Reconciler = require('react-reconciler')
import { DefaultEventPriority, LegacyRoot } from 'react-reconciler/constants'

import {
  appendChild,
  appendChildToContainer,
  appendInitialChild,
  clearContainer,
  commitHostUpdate,
  createDesktopHostContainer,
  createHostContainer,
  createHostInstance,
  createHostTextInstance,
  detachDeletedHostInstance,
  flushStaticTree,
  insertBefore,
  insertInContainerBefore,
  prepareHostUpdate,
  removeChild,
  removeChildFromContainer,
  updateTextInstance,
  type HostContainer,
  type HostContainerOptions,
  type HostInstance,
  type HostUpdatePayload,
  type DesktopRenderHost,
  type RenderHost,
} from './host'

export interface AttoRootOptions extends HostContainerOptions {}

export interface AttoRoot {
  readonly container: HostContainer
  render(element: ReactNode): void
}

type HostContext = null
type UpdatePayload = HostUpdatePayload | null

const hostConfig = {
  now: Date.now,
  supportsMutation: true,
  supportsPersistence: false,
  supportsHydration: false,
  isPrimaryRenderer: false,
  noTimeout: -1,
  scheduleTimeout: setTimeout,
  cancelTimeout: clearTimeout,
  supportsMicrotasks: true,
  scheduleMicrotask,

  getRootHostContext(_container: HostContainer): HostContext {
    return null
  },

  getChildHostContext(_parentHostContext: HostContext, _type: string): HostContext {
    return null
  },

  getPublicInstance(instance: HostInstance): HostInstance {
    return instance
  },

  prepareForCommit(_container: HostContainer): null {
    return null
  },

  resetAfterCommit(container: HostContainer): void {
    flushStaticTree(container)
  },

  createInstance(
    type: string,
    props: Readonly<Record<string, unknown>>,
    rootContainer: HostContainer,
  ): HostInstance {
    return createHostInstance(rootContainer, type, props)
  },

  appendInitialChild(parentInstance: HostInstance, child: HostInstance): void {
    appendInitialChild(parentInstance, child)
  },

  finalizeInitialChildren(
    _instance: HostInstance,
    _type: string,
    _props: Readonly<Record<string, unknown>>,
  ): false {
    return false
  },

  prepareUpdate(
    _instance: HostInstance,
    _type: string,
    oldProps: Readonly<Record<string, unknown>>,
    newProps: Readonly<Record<string, unknown>>,
  ): UpdatePayload {
    return prepareHostUpdate(oldProps, newProps)
  },

  shouldSetTextContent(_type: string, _props: Readonly<Record<string, unknown>>): false {
    return false
  },

  createTextInstance(text: string, rootContainer: HostContainer): HostInstance {
    return createHostTextInstance(rootContainer, text)
  },

  appendChild(parentInstance: HostInstance, child: HostInstance): void {
    appendChild(parentInstance, child)
  },

  appendChildToContainer(container: HostContainer, child: HostInstance): void {
    appendChildToContainer(container, child)
  },

  insertBefore(parentInstance: HostInstance, child: HostInstance, beforeChild: HostInstance): void {
    insertBefore(parentInstance, child, beforeChild)
  },

  insertInContainerBefore(
    container: HostContainer,
    child: HostInstance,
    beforeChild: HostInstance,
  ): void {
    insertInContainerBefore(container, child, beforeChild)
  },

  removeChild(parentInstance: HostInstance, child: HostInstance): void {
    removeChild(parentInstance, child)
  },

  removeChildFromContainer(container: HostContainer, child: HostInstance): void {
    removeChildFromContainer(container, child)
  },

  clearContainer(container: HostContainer): boolean {
    return clearContainer(container)
  },

  commitTextUpdate(textInstance: HostInstance, _oldText: string, newText: string): void {
    updateTextInstance(textInstance, newText)
  },

  commitMount(): void {},

  commitUpdate(instance: HostInstance, updatePayload: UpdatePayload): void {
    if (updatePayload) {
      commitHostUpdate(instance, updatePayload)
    }
  },

  resetTextContent(): void {},

  hideInstance(): void {},

  hideTextInstance(): void {},

  unhideInstance(): void {},

  unhideTextInstance(): void {},

  detachDeletedInstance(instance: HostInstance): void {
    detachDeletedHostInstance(instance)
  },

  preparePortalMount(): void {},

  prepareScopeUpdate(): void {},

  getInstanceFromScope(): null {
    return null
  },

  getInstanceFromNode(): null {
    return null
  },

  beforeActiveInstanceBlur(): void {},

  afterActiveInstanceBlur(): void {},

  getCurrentEventPriority(): number {
    return DefaultEventPriority
  },

  shouldAttemptEagerTransition(): false {
    return false
  },
}

const reconciler = Reconciler(hostConfig)

/** Create a React root bound to an existing atto-ui window. */
export function createRoot(
  host: RenderHost,
  windowId: string,
  options: AttoRootOptions = {},
): AttoRoot {
  const container = createHostContainer(host, windowId, options)
  const root = reconciler.createContainer(
    container,
    LegacyRoot,
    null,
    false,
    null,
    '',
    reportRecoverableError,
    null,
  )

  return {
    container,
    render(element: ReactNode): void {
      reconciler.updateContainer(element, root, null, null)
    },
  }
}

/** Create a React root bound to the virtual desktop container. */
export function createDesktopRoot(
  host: DesktopRenderHost,
  options: AttoRootOptions = {},
): AttoRoot {
  const container = createDesktopHostContainer(host, options)
  const root = reconciler.createContainer(
    container,
    LegacyRoot,
    null,
    false,
    null,
    '',
    reportRecoverableError,
    null,
  )

  return {
    container,
    render(element: ReactNode): void {
      reconciler.updateContainer(element, root, null, null)
    },
  }
}

/** Render once into an existing atto-ui window and return the created root. */
export function renderToWindow(
  element: ReactNode,
  host: RenderHost,
  windowId: string,
  options: AttoRootOptions = {},
): AttoRoot {
  const root = createRoot(host, windowId, options)
  root.render(element)
  return root
}

function scheduleMicrotask(callback: () => void): void {
  if (typeof queueMicrotask === 'function') {
    queueMicrotask(callback)
  } else {
    Promise.resolve().then(callback)
  }
}

function reportRecoverableError(error: unknown): void {
  if (error instanceof Error) {
    throw error
  }
  throw new Error(String(error))
}
