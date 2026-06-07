'use strict'

const core = require('..')

const host = new core.AppHost({ tickRate: 0 })
const windowId = host.addDynamicWindow('Runtime Compat', { x: 1, y: 1, width: 42, height: 8 }, {
  type: 'Label',
  id: 'compat-ready',
  props: { text: 'Runtime PTY Ready' },
})

let stopped = false

function cleanup() {
  if (stopped) return
  stopped = true
  try {
    host.closeWindow(windowId)
  } catch {
    // The host may already have closed during Ctrl+Q shutdown.
  }
  host.dispose()
}

function scheduleTick() {
  if (typeof setImmediate === 'function') {
    setImmediate(tick)
  } else {
    setTimeout(tick, 0)
  }
}

function tick() {
  if (stopped) return
  if (!host.step()) {
    cleanup()
    return
  }
  scheduleTick()
}

process.once('SIGINT', () => {
  cleanup()
  process.exit(0)
})
process.once('SIGTERM', () => {
  cleanup()
  process.exit(0)
})
process.once('exit', cleanup)

scheduleTick()
