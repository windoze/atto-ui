'use strict'

const assert = require('node:assert/strict')
const {
  Button,
  Grid,
  RichText,
  TabView,
  Text,
  TextSpan,
  VStack,
  child,
  component,
  tab,
} = require('..')

const root = VStack({ id: 'root', spacing: 2, padding: 1 }, [
  Text('Hello', { id: 'hello', selectable: true }),
  Button({ id: 'send', text: 'Send', onClick: 'atto:callback:1', disabled: false }),
  child(Grid({ columns: 2, rowGap: 1, columnGap: 3 }, [Text('Nested')]), {
    layout: { width: 'fill' },
    meta: { slot: 'main' },
  }),
])

assert.deepStrictEqual(root, {
  type: 'VStack',
  id: 'root',
  props: { spacing: 2, padding: 1 },
  children: [
    { type: 'Text', id: 'hello', props: { text: 'Hello', selectable: true } },
    {
      type: 'Button',
      id: 'send',
      props: { label: 'Send', enabled: true },
      events: { click: 'atto:callback:1' },
    },
    {
      node: {
        type: 'Grid',
        props: { columns: 2, row_gap: 1, column_gap: 3 },
        children: [{ type: 'Text', props: { text: 'Nested' } }],
      },
      layout: { width: 'fill' },
      meta: { slot: 'main' },
    },
  ],
})

assert.deepStrictEqual(
  component('CustomWidget', {
    id: 'custom',
    props: { title: 'Raw', omitted: undefined },
    events: { activate: 'atto:callback:2', skipped: undefined },
  }),
  {
    type: 'CustomWidget',
    id: 'custom',
    props: { title: 'Raw' },
    events: { activate: 'atto:callback:2' },
  },
)

assert.deepStrictEqual(Button({ label: 'Explicit', events: { click: 'atto:callback:4' } }), {
  type: 'Button',
  props: { label: 'Explicit' },
  events: { click: 'atto:callback:4' },
})

assert.deepStrictEqual(
  RichText([TextSpan('A', { bold: true }), TextSpan('B', { href: 'https://example.test' })]),
  {
    type: 'RichText',
    children: [
      { type: 'TextSpan', props: { text: 'A', bold: true } },
      { type: 'TextSpan', props: { text: 'B', href: 'https://example.test' } },
    ],
  },
)

assert.deepStrictEqual(
  TabView({ selection: 1 }, [tab('One', Text('First')), tab('Two', Text('Second'))]),
  {
    type: 'TabView',
    props: { selection: 1 },
    children: [
      { node: { type: 'Text', props: { text: 'First' } }, meta: { title: 'One' } },
      { node: { type: 'Text', props: { text: 'Second' } }, meta: { title: 'Two' } },
    ],
  },
)
