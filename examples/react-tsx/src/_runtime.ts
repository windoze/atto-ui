import type { ReactNode } from 'react'
import { render, type RenderHandle, type RenderOptions } from '@atto-ui/react'

export type SnapshotNode = {
  readonly id?: string
  readonly text?: string
  readonly name?: string
  readonly children?: readonly SnapshotNode[]
}

/** True when the demo runs under the deterministic in-memory terminal. */
export function isHeadless(): boolean {
  return process.env.ATTO_UI_EXAMPLE_HEADLESS === '1'
}

export function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

/** Collect every non-empty visible text label from an AppHost snapshot tree. */
export function collectTexts(node: SnapshotNode, out: string[] = []): string[] {
  if (typeof node.text === 'string' && node.text.length > 0) out.push(node.text)
  for (const child of node.children ?? []) collectTexts(child, out)
  return out
}

export function hasText(handle: RenderHandle, expected: string): boolean {
  return collectTexts(handle.host.snapshot().tree as SnapshotNode).some((text) => text.includes(expected))
}

/** Poll until `predicate` is satisfied or fail with a descriptive label. */
export async function waitFor(predicate: () => boolean, label: string, timeoutMs = 1500): Promise<void> {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    if (predicate()) return
    await delay(10)
  }
  throw new Error(`timed out waiting for ${label}`)
}

export function sendKey(handle: RenderHandle, windowId: string, key: string): void {
  handle.host.sendEvent(windowId, { type: 'key', key })
}

export function sendChar(handle: RenderHandle, windowId: string, char: string): void {
  handle.host.sendEvent(handle.windowIds()[0] ?? windowId, { type: 'key', char })
}

export interface DemoOptions extends RenderOptions {
  /** Headless probe drives synthetic events and asserts the resulting snapshot. */
  readonly headlessProbe?: (handle: RenderHandle) => Promise<void>
}

/**
 * Render a demo. Interactive mode runs the terminal UI until `Ctrl+Q`. Headless
 * mode drives the optional probe, prints a compact snapshot, then exits.
 */
export function startDemo(element: ReactNode, options: DemoOptions = {}): void {
  const headless = isHeadless()
  const { headlessProbe, ...renderOptions } = options
  const handle = render(element, {
    cols: 80,
    rows: 24,
    ...renderOptions,
    headless,
  })

  if (!headless) return

  void (async () => {
    try {
      await headlessProbe?.(handle)
      const texts = collectTexts(handle.host.snapshot().tree as SnapshotNode)
      console.log('--- headless snapshot ---')
      for (const text of texts.slice(0, 20)) {
        console.log(`  ${text.length > 120 ? `${text.slice(0, 117)}...` : text}`)
      }
    } catch (error) {
      console.error('headless probe failed:', error)
      process.exitCode = 1
    } finally {
      handle.stop()
    }
  })()
}
