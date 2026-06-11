/**
 * 10 — Global state (Zustand)
 *
 * Zustand has no DOM dependency, so it works in the terminal with no bridging:
 * `create()` a store, read it with selectors, and call actions from atto-ui
 * handlers. The store can also be driven from *outside* React — handy for the
 * non-blocking tick loop (timers, streams, async tasks) without prop drilling.
 * Run interactively:  npm run zustand
 * Headless smoke:      ATTO_UI_EXAMPLE_HEADLESS=1 npm run zustand
 */
import { useEffect } from 'react'
import { create } from 'zustand'
import { Button, Divider, Text, VStack, Window } from '@atto-ui/react'

import { hasText, sendKey, startDemo, waitFor } from './_runtime'

type Store = {
  count: number
  streamed: string
  inc: () => void
}

const useStore = create<Store>()((set) => ({
  count: 0,
  streamed: '',
  inc: () => set((s) => ({ count: s.count + 1 })),
}))

// Outside React: feed tokens into the store imperatively, like a background
// stream would. Components subscribed via selectors re-render automatically.
function startStream(): () => void {
  const tokens = ['the ', 'quick ', 'brown ', 'fox']
  let i = 0
  const id = setInterval(() => {
    if (i >= tokens.length) {
      clearInterval(id)
      return
    }
    useStore.setState((s) => ({ streamed: s.streamed + tokens[i] }))
    i += 1
  }, 30)
  return () => clearInterval(id)
}

function App() {
  const count = useStore((s) => s.count)
  const streamed = useStore((s) => s.streamed)
  const inc = useStore((s) => s.inc)

  useEffect(() => startStream(), [])

  return (
    <Window title="Zustand" rect={[2, 1, 44, 10]}>
      <VStack spacing={1} padding={1}>
        <Text>{`Count: ${count}`}</Text>
        <Button onClick={inc}>Increment</Button>
        <Divider />
        <Text>{`Stream: ${streamed}`}</Text>
      </VStack>
    </Window>
  )
}

startDemo(<App />, {
  singleWindow: false,
  idPrefix: 'zustand',
  async headlessProbe(handle) {
    const windowId = handle.windowIds()[0]!
    await waitFor(() => hasText(handle, 'Count: 0'), 'initial count')
    sendKey(handle, windowId, 'enter')
    await waitFor(() => hasText(handle, 'Count: 1'), 'count after click')
    // The background stream updates the store from outside React.
    await waitFor(() => hasText(handle, 'Stream: the quick brown fox'), 'external store stream')
  },
})
