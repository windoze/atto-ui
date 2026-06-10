import type { CallbackInvocation, ComponentValue, WindowEvent } from '@atto-ui/core'

export interface AttoUiCallbackEvent {
  readonly callbackId: string
  readonly targetId: string | null
  readonly event: string
  readonly payload: ComponentValue | null
  readonly nativeEvent: CallbackInvocation
}

export type AttoUiEventHandler = (event: AttoUiCallbackEvent) => void

export type AttoUiWindowEvent = WindowEvent
export type AttoUiWindowEventHandler = (event: AttoUiWindowEvent) => void

export interface CallbackEventDispatcherOptions {
  allocCallback(): string
  releaseCallback?(callbackId: string): boolean
}

/** Tracks native callback handles and dispatches drained runtime callbacks to React handlers. */
export class CallbackEventDispatcher {
  private readonly handlers = new Map<string, unknown>()
  private readonly allocCallback: () => string
  private readonly releaseCallback?: (callbackId: string) => boolean

  constructor(options: CallbackEventDispatcherOptions) {
    this.allocCallback = options.allocCallback
    this.releaseCallback = options.releaseCallback
  }

  register(handler: unknown): string {
    const callbackId = this.allocCallback()
    this.handlers.set(callbackId, handler)
    return callbackId
  }

  update(callbackId: string, handler: unknown): void {
    if (this.handlers.has(callbackId)) {
      this.handlers.set(callbackId, handler)
    }
  }

  unregister(callbackId: string): void {
    if (!this.handlers.delete(callbackId)) return
    this.releaseCallback?.(callbackId)
  }

  dispatch(invocation: CallbackInvocation): boolean {
    const handler = this.handlers.get(invocation.callbackId)
    if (typeof handler !== 'function') return false
    const callback = handler as AttoUiEventHandler
    callback(callbackEvent(invocation))
    return true
  }

  dispatchAll(invocations: readonly CallbackInvocation[]): number {
    let dispatched = 0
    for (const invocation of invocations) {
      if (this.dispatch(invocation)) {
        dispatched += 1
      }
    }
    return dispatched
  }

  has(callbackId: string): boolean {
    return this.handlers.has(callbackId)
  }
}

function callbackEvent(invocation: CallbackInvocation): AttoUiCallbackEvent {
  return {
    callbackId: invocation.callbackId,
    targetId: invocation.targetId,
    event: invocation.event,
    payload: invocation.payload,
    nativeEvent: invocation,
  }
}
