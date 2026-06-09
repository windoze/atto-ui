/**
 * 01 — Hello window
 *
 * The smallest possible atto-ui React app: one window with static labels.
 * Run interactively:  npm run hello
 * Headless smoke:      ATTO_UI_EXAMPLE_HEADLESS=1 npm run hello
 */
import { Text, B, VStack, Window } from '@atto-ui/react'

import { startDemo, waitFor, hasText } from './_runtime'

function App() {
  return (
    <Window title="Hello" rect={[2, 1, 40, 8]}>
      <VStack spacing={1} padding={1}>
        <Text>Welcome to atto-ui + React.</Text>
        <Text>
          Press <B>Ctrl+Q</B> to quit.
        </Text>
      </VStack>
    </Window>
  )
}

startDemo(<App />, {
  singleWindow: false,
  idPrefix: 'hello',
  async headlessProbe(handle) {
    await waitFor(() => hasText(handle, 'Welcome to atto-ui'), 'welcome label')
  },
})
