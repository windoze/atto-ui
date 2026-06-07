const React = require('react')
const { Button, ListBox, TextBox, VStack, Window, render } = require('../dist')

function E2eApp() {
  const [count, setCount] = React.useState(0)
  const [draft, setDraft] = React.useState('')
  const [items, setItems] = React.useState(['Seed'])
  const [selectedIndex, setSelectedIndex] = React.useState(0)
  const [lastAction, setLastAction] = React.useState('ready')

  const selectedItem = items[selectedIndex] ?? 'none'
  const addedItem = lastAction.startsWith('added ') ? lastAction.slice('added '.length) : ''
  const removedItem = lastAction.startsWith('removed ') ? lastAction.slice('removed '.length) : ''

  function addItem() {
    const label = draft.trim() || `Item ${items.length + 1}`
    setItems([...items, label])
    setSelectedIndex(items.length)
    setDraft('')
    setLastAction(`added ${label}`)
  }

  function removeSelected() {
    if (items.length === 0) {
      setLastAction('empty')
      return
    }

    const removeIndex = Math.min(selectedIndex, items.length - 1)
    const removed = items[removeIndex]
    const nextItems = items.filter((_, index) => index !== removeIndex)
    setItems(nextItems)
    setSelectedIndex(Math.max(0, Math.min(removeIndex, nextItems.length - 1)))
    setLastAction(`removed ${removed}`)
  }

  return React.createElement(
    React.Fragment,
    null,
    React.createElement(
      Window,
      { title: 'E2E Summary', rect: [1, 1, 29, 19] },
      React.createElement(
        VStack,
        null,
        React.createElement('label', { text: 'Summary Window' }),
        React.createElement('label', { text: `Counter: ${count}` }),
        React.createElement('label', { text: draft ? `Draft typed: ${draft}` : '' }),
        React.createElement('label', { text: addedItem ? `Added item: ${addedItem}` : '' }),
        React.createElement('label', { text: removedItem ? `Removed item: ${removedItem}` : '' }),
        React.createElement('label', { text: count > 0 ? `Counter clicked: ${count}` : '' }),
        React.createElement('label', { text: `Items total: ${items.length}` }),
        React.createElement('label', { text: `Selected: ${selectedItem}` }),
      ),
    ),
    React.createElement(
      Window,
      { title: 'E2E Actions', rect: [32, 1, 46, 18] },
      React.createElement(
        VStack,
        null,
        React.createElement(TextBox, {
          title: 'Name',
          value: draft,
          onChange(value) {
            setDraft(value)
          },
        }),
        React.createElement(Button, { onClick: addItem }, 'Add item'),
        React.createElement(Button, { onClick: removeSelected }, 'Remove selected'),
        React.createElement(Button, {
          onClick() {
            setCount((current) => current + 1)
          },
        }, 'Increment counter'),
        React.createElement(ListBox, {
          title: 'Items',
          height: 5,
          items,
          selectedIndex,
          onSelect(index) {
            setSelectedIndex(index)
          },
        }),
      ),
    ),
  )
}

if (require.main === module) {
  render(React.createElement(E2eApp), {
    singleWindow: false,
    idPrefix: 'e2e-pty',
  })
}

module.exports = { E2eApp }
