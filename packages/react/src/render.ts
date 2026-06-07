import { createElement, type ReactNode } from 'react'
import { AppHost, type AppHostConfig, type Rect } from '@atto-ui/core'

import { dispatchHostCallbacks } from './host'
import { Window } from './desktop'
import { createDesktopRoot, type AttoRoot, type AttoRootOptions } from './reconciler'

export interface RenderOptions extends AttoRootOptions {
  readonly cols?: number
  readonly rows?: number
  readonly singleWindow?: boolean
  readonly headless?: boolean
}

export interface RenderHandle {
  readonly host: AppHost
  readonly root: AttoRoot
  readonly windowId: string | null
  windowIds(): string[]
  stop(): void
}

/** Render a React tree into the virtual desktop container and start the non-blocking tick loop. */
export function render(element: ReactNode, options: RenderOptions = {}): RenderHandle {
  const host = new AppHost(appHostConfig(options))

  try {
    const bounds = host.snapshot().bounds
    const root = createDesktopRoot(host, { idPrefix: options.idPrefix })
    root.render(renderElementForOptions(element, options, bounds))
    return startTickLoop(host, root)
  } catch (error) {
    cleanupHost(host, null)
    throw error
  }
}

function appHostConfig(options: RenderOptions): AppHostConfig {
  return {
    headless: options.headless ?? false,
    cols: options.cols,
    rows: options.rows,
    tickRate: 0,
  }
}

function fullScreenRect(bounds: Rect): Rect {
  return {
    x: bounds.x,
    y: bounds.y,
    width: bounds.width,
    height: bounds.height,
  }
}

function renderElementForOptions(element: ReactNode, options: RenderOptions, bounds: Rect): ReactNode {
  if (options.singleWindow === false) {
    return element
  }
  return createElement(Window, {
    title: 'atto-ui React',
    rect: fullScreenRect(bounds),
  }, element)
}

function startTickLoop(host: AppHost, root: AttoRoot): RenderHandle {
  let active = true
  let scheduled: NodeJS.Immediate | null = null

  const stop = (): void => {
    if (!active) return
    active = false
    if (scheduled !== null) {
      clearImmediate(scheduled)
      scheduled = null
    }
    cleanupHost(host, root)
  }

  const tick = (): void => {
    scheduled = null
    if (!active) return

    try {
      if (!host.step()) {
        stop()
        return
      }
      dispatchHostCallbacks(root.container, host.drainCallbacks())
    } catch (error) {
      try {
        stop()
      } catch {
        // Preserve the original tick failure; cleanup errors are secondary here.
      }
      throw error
    }

    if (active) {
      scheduled = setImmediate(tick)
    }
  }

  scheduled = setImmediate(tick)

  return {
    host,
    root,
    get windowId(): string | null {
      return windowIdsFromRoot(root)[0] ?? null
    },
    windowIds(): string[] {
      return windowIdsFromRoot(root)
    },
    stop,
  }
}

function cleanupHost(host: AppHost, root: AttoRoot | null): void {
  let cleanupError: unknown

  if (root !== null) {
    try {
      root.render(null)
    } catch (error) {
      cleanupError = error
    }
  }

  for (const window of host.listWindows()) {
    try {
      host.closeWindow(window.id)
    } catch (error) {
      cleanupError ??= error
    }
  }

  try {
    host.dispose()
  } catch (error) {
    cleanupError ??= error
  }

  if (cleanupError !== undefined) {
    throw cleanupError
  }
}

function windowIdsFromRoot(root: AttoRoot): string[] {
  return root.container.rootChildren
    .map((child) => child.windowId)
    .filter((windowId): windowId is string => windowId !== null)
}
