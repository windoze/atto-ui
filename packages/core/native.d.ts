declare class NativeAppHost {
  constructor(config?: object | null)
  addDynamicWindow(title: string, rect: object | readonly [number, number, number, number], root: unknown): string
  applyTreeOps(windowId: string, ops: unknown): boolean
  step(): boolean
  dispose(): void
  drainCallbacks(): unknown
  allocCallback(): string
  releaseCallback(callbackId: string): boolean
  sendEvent(windowId: string, event: unknown): unknown
  closeWindow(windowId: string): boolean
  focusWindow(windowId: string): boolean
  moveWindow(windowId: string, x: number, y: number): boolean
  resizeWindow(windowId: string, width: number, height: number): boolean
  listWindows(): unknown
  setTitle(windowId: string, title: string): boolean
  setMenuBar(spec: unknown): void
  setStatusBar(left?: string | null, right?: string | null): void
  setProperty(id: string, name: string, value: unknown): void
  getProperty(id: string, name: string): unknown
  snapshot(): unknown
  setTheme(name: string): void
  loadTheme(path: string, base?: string | null): void
  schemas(): unknown
}

declare const native: {
  readonly AppHost: typeof NativeAppHost
  readonly registerAllRuntimeComponents: () => void
  readonly version: () => string
}

export = native
