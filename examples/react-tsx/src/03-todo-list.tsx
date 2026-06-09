/**
 * 03 — Todo list
 *
 * Controlled `TextBox` input plus a `ListBox` with add/remove buttons.
 * Shows how React state flows into native widgets and back through events.
 * Run interactively:  npm run todo
 * Headless smoke:      ATTO_UI_EXAMPLE_HEADLESS=1 npm run todo
 */
import { useState } from 'react'
import { Button, ListBox, Text, TextBox, VStack, Window } from '@atto-ui/react'

import { startDemo, sendKey, sendChar, waitFor, hasText } from './_runtime'

function App() {
  const [draft, setDraft] = useState('')
  const [items, setItems] = useState<string[]>(['Read the docs'])
  const [selected, setSelected] = useState(0)

  function addItem() {
    const label = draft.trim() || `Item ${items.length + 1}`
    setItems([...items, label])
    setSelected(items.length)
    setDraft('')
  }

  function removeSelected() {
    if (items.length === 0) return
    const next = items.filter((_, index) => index !== selected)
    setItems(next)
    setSelected(Math.max(0, Math.min(selected, next.length - 1)))
  }

  return (
    <Window title="Todos" rect={[2, 1, 44, 16]}>
      <VStack spacing={1} padding={1}>
        <TextBox title="New todo" value={draft} onChange={setDraft} />
        <Button onClick={addItem}>Add</Button>
        <Button onClick={removeSelected}>Remove selected</Button>
        <ListBox title="Items" height={6} items={items} selectedIndex={selected} onSelect={setSelected} />
        <Text>{`Draft: ${draft}`}</Text>
        <Text>{`Total: ${items.length}`}</Text>
      </VStack>
    </Window>
  )
}

startDemo(<App />, {
  singleWindow: false,
  idPrefix: 'todo',
  async headlessProbe(handle) {
    const windowId = handle.windowIds()[0]!
    await waitFor(() => hasText(handle, 'Total: 1'), 'initial total')
    // Type into the focused TextBox, then Tab to the Add button and press Enter.
    sendChar(handle, windowId, 'q')
    sendChar(handle, windowId, 'a')
    await waitFor(() => hasText(handle, 'Draft: qa'), 'typed draft mirrored to state')
    sendKey(handle, windowId, 'tab')
    sendKey(handle, windowId, 'enter')
    await waitFor(() => hasText(handle, 'Total: 2'), 'total after add')
  },
})
