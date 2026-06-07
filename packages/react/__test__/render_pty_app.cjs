const React = require('react')
const { render } = require('../dist')

render(
  React.createElement('label', { text: 'React PTY Ready' }),
  { idPrefix: 'pty-render' },
)
