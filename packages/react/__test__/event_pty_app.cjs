const React = require('react')
const { render } = require('../dist')

function CounterButton() {
  const [count, setCount] = React.useState(0)
  return React.createElement(
    'vstack',
    null,
    React.createElement('button', {
      label: count === 0 ? 'Push Button' : `Clicked Button ${count}`,
      onClick() {
        setCount((current) => current + 1)
      },
    }),
  )
}

render(
  React.createElement(CounterButton),
  { idPrefix: 'event-pty' },
)
