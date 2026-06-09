/**
 * 04 — Desktop: multiple windows, menu bar, status bar
 *
 * The desktop root can hold several `Window`s plus the `MenuBar` and
 * `StatusBar` slots. Menu items and buttons share the same event model.
 * Run interactively:  npm run desktop
 * Headless smoke:      ATTO_UI_EXAMPLE_HEADLESS=1 npm run desktop
 */
import { useState } from 'react'
import { Button, MenuBar, Menu, MenuItem, StatusBar, Text, VStack, Window } from '@atto-ui/react'

import { startDemo, waitFor, hasText } from './_runtime'

function App() {
  const [log, setLog] = useState('ready')

  return (
    <>
      <MenuBar>
        <Menu title="File">
          <MenuItem label="New" shortcut="Ctrl+N" onClick={() => setLog('menu: New')} />
          <MenuItem label="Quit" shortcut="Ctrl+Q" onClick={() => setLog('menu: Quit')} />
        </Menu>
        <Menu title="Help">
          <MenuItem label="About" onClick={() => setLog('menu: About')} />
        </Menu>
      </MenuBar>

      <Window title="Main" rect={[1, 1, 34, 9]}>
        <VStack spacing={1} padding={1}>
          <Text>Two windows share one desktop.</Text>
          <Button onClick={() => setLog('button: Ping')}>Ping</Button>
        </VStack>
      </Window>

      <Window title="Activity" rect={[37, 1, 34, 9]}>
        <VStack spacing={1} padding={1}>
          <Text>{`Last action: ${log}`}</Text>
        </VStack>
      </Window>

      <StatusBar left="atto-ui desktop" right={log} />
    </>
  )
}

startDemo(<App />, {
  singleWindow: false,
  idPrefix: 'desktop',
  cols: 80,
  rows: 24,
  async headlessProbe(handle) {
    await waitFor(() => handle.windowIds().length === 2, 'two windows')
    await waitFor(() => hasText(handle, 'Last action: ready'), 'initial activity')
    const mainWindowId = handle.windowIds()[0]!
    handle.host.sendEvent(mainWindowId, { type: 'key', key: 'enter' })
    await waitFor(() => hasText(handle, 'Last action: button: Ping'), 'button updates other window')
  },
})
