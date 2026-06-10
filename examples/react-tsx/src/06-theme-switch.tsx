/**
 * 06 — Theme switching
 *
 * The React tree has no theme prop. Themes live on the AppHost that `render()`
 * returns: call `handle.host.setTheme('dark' | 'light' | 'turbo')` (or
 * `handle.host.loadTheme(path, base)` for a JSON/YAML file). The tick loop
 * repaints with the new theme on the next frame.
 *
 * Here a Button cycles dark → light → turbo, updating React state for the
 * visible label and the host theme for the actual skin.
 * Run interactively:  npm run theme   (Tab to focus, Enter/Space to cycle)
 * Headless smoke:      ATTO_UI_EXAMPLE_HEADLESS=1 npm run theme
 */
import { useState } from 'react'
import { Button, Text, VStack, Window } from '@atto-ui/react'

import { startDemo, sendKey, waitFor, hasText } from './_runtime'

const THEMES = ['dark', 'light', 'turbo'] as const
type ThemeName = (typeof THEMES)[number]

// Bridges the React tree to the host. The AppHost does not exist while the
// component is first defined, so we capture it after `startDemo` returns and
// invoke it from the Button handler.
let applyTheme: (name: ThemeName) => void = () => {}

function App() {
  const [theme, setTheme] = useState<ThemeName>('dark')

  const cycle = () => {
    const next = THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length]
    setTheme(next)
    applyTheme(next)
  }

  return (
    <Window title="Theme Switch" rect={[2, 1, 40, 8]}>
      <VStack spacing={1} padding={1}>
        <Text>{`Theme: ${theme}`}</Text>
        <Button onClick={cycle}>Cycle theme</Button>
      </VStack>
    </Window>
  )
}

const handle = startDemo(<App />, {
  singleWindow: false,
  idPrefix: 'theme',
  async headlessProbe(h) {
    const windowId = h.windowIds()[0]!
    await waitFor(() => hasText(h, 'Theme: dark'), 'initial theme')
    sendKey(h, windowId, 'enter')
    await waitFor(() => hasText(h, 'Theme: light'), 'theme after first cycle')
    sendKey(h, windowId, 'enter')
    await waitFor(() => hasText(h, 'Theme: turbo'), 'theme after second cycle')
  },
})

applyTheme = (name) => handle.host.setTheme(name)
