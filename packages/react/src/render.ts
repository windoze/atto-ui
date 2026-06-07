import type { ReactNode } from 'react'
import { AppHost, type AppHostConfig, type Rect } from '@atto-ui/core'

import { createRoot, type AttoRoot, type AttoRootOptions } from './reconciler'

export interface RenderOptions extends AttoRootOptions {
  readonly cols?: number
  readonly rows?: number
  readonly singleWindow?: boolean
  readonly headless?: boolean
}

export interface RenderHandle {
  readonly host: AppHost
  readonly root: AttoRoot
  readonly windowId: string
  stop(): void
}

/** Render a React tree into a single atto-ui window and start the non-blocking tick loop. */
export function render(element: ReactNode, options: RenderOptions = {}): RenderHandle {
  if (options.singleWindow === false) {
    throw new Error('render() supports only singleWindow mode until DesktopContainer support lands')
  }

  const host = new AppHost(appHostConfig(options))
  let windowId: string | null = null

  try {
    const bounds = host.snapshot().bounds
    windowId = host.addDynamicWindow('atto-ui React', fullScreenRect(bounds), emptyRootSpec(options))
    const root = createRoot(host, windowId, { idPrefix: options.idPrefix })
    root.render(element)
    return startTickLoop(host, root, windowId)
  } catch (error) {
    cleanupHost(host, null, windowId)
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

function emptyRootSpec(options: RenderOptions) {
  return {
    type: 'Spacer',
    id: `${options.idPrefix ?? 'atto-react-render'}-empty-root`,
  }
}

function startTickLoop(host: AppHost, root: AttoRoot, windowId: string): RenderHandle {
  let active = true
  let scheduled: NodeJS.Immediate | null = null

  const stop = (): void => {
    if (!active) return
    active = false
    if (scheduled !== null) {
      clearImmediate(scheduled)
      scheduled = null
    }
    cleanupHost(host, root, windowId)
  }

  const tick = (): void => {
    scheduled = null
    if (!active) return

    try {
      if (!host.step()) {
        stop()
        return
      }
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

  return { host, root, windowId, stop }
}

function cleanupHost(host: AppHost, root: AttoRoot | null, windowId: string | null): void {
  let cleanupError: unknown

  if (root !== null) {
    try {
      root.render(null)
    } catch (error) {
      cleanupError = error
    }
  }

  if (windowId !== null) {
    try {
      host.closeWindow(windowId)
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
