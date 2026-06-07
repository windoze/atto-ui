const React = require('react')
const { Button, ListBox, Table, TextBox, VStack, render } = require('../dist')

function ComponentsApp() {
  const [text, setText] = React.useState('')
  const [clicks, setClicks] = React.useState(0)
  const [listSelection, setListSelection] = React.useState(0)
  const [tableSelection, setTableSelection] = React.useState(0)
  const listItems = ['Alpha', 'Beta']
  const tableRows = [['Row A'], ['Row B']]

  return React.createElement(
    VStack,
    null,
    React.createElement(TextBox, {
      title: 'Name',
      value: text,
      onChange(value) {
        setText(value)
      },
    }),
    React.createElement('label', { text: text ? `Typed: ${text}` : '' }),
    React.createElement(Button, {
      onClick() {
        setClicks((current) => current + 1)
      },
    }, 'Push'),
    React.createElement('label', { text: clicks > 0 ? `Button: ${clicks}` : '' }),
    React.createElement(ListBox, {
      title: 'List',
      height: 3,
      items: listItems,
      selectedIndex: listSelection,
      onSelect(index) {
        setListSelection(index)
      },
    }),
    React.createElement('label', { text: listSelection > 0 ? `List: ${listItems[listSelection]}` : '' }),
    React.createElement('label', { text: tableSelection > 0 ? `Table: ${tableRows[tableSelection][0]}` : '' }),
    React.createElement(Table, {
      title: 'Table',
      height: 4,
      headers: ['Name'],
      rows: tableRows,
      selectedIndex: tableSelection,
      onSelect(index) {
        setTableSelection(index)
      },
    }),
  )
}

render(
  React.createElement(ComponentsApp),
  { idPrefix: 'components-pty' },
)
