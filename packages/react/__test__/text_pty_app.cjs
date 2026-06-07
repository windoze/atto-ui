const React = require('react')
const { Link, Text, render } = require('../dist')

function LinkText() {
  const [clicked, setClicked] = React.useState(false)
  return React.createElement(
    'vstack',
    null,
    React.createElement(
      Text,
      null,
      'Docs: ',
      React.createElement(Link, {
        href: 'https://example.com/docs',
        onClick() {
          setClicked(true)
        },
      }, 'Open Docs'),
    ),
    React.createElement('label', { text: clicked ? 'Link Clicked' : 'Waiting Link' }),
  )
}

render(
  React.createElement(LinkText),
  { idPrefix: 'text-pty' },
)
