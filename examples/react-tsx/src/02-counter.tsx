/**
 * 02 — Counter
 *
 * React state (`useState`) drives a label; a Button click updates it.
 * Run interactively:  npm run counter   (Tab to focus, Enter/Space to click)
 * Headless smoke:      ATTO_UI_EXAMPLE_HEADLESS=1 npm run counter
 */
import { useState } from 'react'
import { Button, Text, VStack, Window } from '@atto-ui/react'

import { startDemo, sendKey, waitFor, hasText } from './_runtime'

function App() {
  const [count, setCount] = useState(0)

  return (
    <Window title="Counter" rect={[2, 1, 36, 8]}>
      <VStack spacing={1} padding={1}>
        <Text>{`Count: ${count}`}</Text>
        <Button onClick={() => setCount((n) => n + 1)}>Increment</Button>
        <Button onClick={() => setCount(0)}>Reset</Button>
      </VStack>
    </Window>
  )
}

startDemo(<App />, {
  singleWindow: false,
  idPrefix: 'counter',
  async headlessProbe(handle) {
    const windowId = handle.windowIds()[0]!
    await waitFor(() => hasText(handle, 'Count: 0'), 'initial count')
    sendKey(handle, windowId, 'enter')
    await waitFor(() => hasText(handle, 'Count: 1'), 'count after click')
    sendKey(handle, windowId, 'enter')
    await waitFor(() => hasText(handle, 'Count: 2'), 'count after second click')
  },
})
