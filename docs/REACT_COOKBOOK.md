# @atto-ui/react Component Cookbook

Focused JSX snippets for each component. For a guided walkthrough start with the
[Getting Started tutorial](./REACT_GETTING_STARTED.md); for full type shapes see
[`NODE_API.md`](./NODE_API.md).

All examples assume imports from `@atto-ui/react`.

## Window

A desktop window. `rect` is `[x, y, width, height]` in terminal cells.

```tsx
<Window title="Editor" rect={[2, 1, 60, 20]}>
  {/* children */}
</Window>
```

Render multiple windows by returning them from the root with
`render(<App />, { singleWindow: false })`.

## VStack / HStack

Vertical and horizontal layout. `spacing` is the gap between children;
`padding` accepts a number or an `EdgeInsets`-like object; `scrollable` adds a
scroll viewport.

```tsx
<VStack spacing={1} padding={1}>
  <Text>Top</Text>
  <Text>Bottom</Text>
</VStack>

<HStack spacing={2}>
  <Button>Left</Button>
  <Button>Right</Button>
</HStack>
```

## Grid

Column-based grid. `columns` sets the column count; `rowGap` / `columnGap`
control spacing.

```tsx
<Grid columns={2} rowGap={1} columnGap={2}>
  <Text>R1C1</Text>
  <Text>R1C2</Text>
  <Text>R2C1</Text>
  <Text>R2C2</Text>
</Grid>
```

## Button

`onClick` fires on Enter/Space (when focused) or mouse click. The label comes
from children or the `label` prop.

```tsx
<Button onClick={() => doThing()}>Save</Button>
<Button label="Cancel" enabled={false} />
```

## TextBox (controlled)

Pass `value` and update it from `onChange`. `onChange` receives the next string
value first; the raw event is the second argument.

```tsx
const [name, setName] = useState('')

<TextBox
  title="Name"
  placeholder="Type here"
  value={name}
  onChange={(value) => setName(value)}
  onSubmit={() => save(name)}
/>
```

The widget's live text is in the runtime tree's `properties.text`. Mirror it
into a `<Text>` node if you want it visible elsewhere.

## ListBox

`items` is the list; `selectedIndex` is the controlled selection; `onSelect`
(or `onChange`) reports the new index. `height` caps the visible rows.

```tsx
const [index, setIndex] = useState(0)

<ListBox
  title="Files"
  height={6}
  items={['a.ts', 'b.ts', 'c.ts']}
  selectedIndex={index}
  onSelect={setIndex}
/>
```

## Table / TableView

`headers` plus a `rows` matrix of strings. Selection mirrors `ListBox`.

```tsx
<Table
  title="People"
  headers={['Name', 'Role']}
  rows={[
    ['Ada', 'Engineer'],
    ['Grace', 'Admiral'],
  ]}
  onSelect={(rowIndex) => inspect(rowIndex)}
/>
```

`TableView` is an alias for `Table`.

## Text with inline styles

`Text` renders a rich-text container. Wrap fragments in `B`/`I`/`U`/`S` for
bold/italic/underline/strikethrough, and `Link` for clickable links.

```tsx
import { Text, B, I, U, S, Link } from '@atto-ui/react'

<Text>
  Plain, <B>bold</B>, <I>italic</I>, <U>underline</U>, <S>strike</S>, and{' '}
  <Link href="https://example.com" onClick={() => open()}>a link</Link>.
</Text>
```

> A single interpolated string renders as one span:
> `<Text>{`Count: ${n}`}</Text>`. Mixing text and expressions
> (`<Text>Count: {n}</Text>`) produces multiple spans.

## Markdown

Block markdown rendering. Pass `markdown` directly, or use children text. Useful
knobs: `wrapWidth`, `showMarkers`, `verticalScrollbar`, `codeBlockMaxHeight`,
`tableMaxHeight`.

```tsx
<Markdown markdown={"# Title\n\n- one\n- two\n\n```ts\nconst x = 1\n```"} />

<Markdown wrapWidth={60} verticalScrollbar="auto">
  {streamedText}
</Markdown>
```

## MenuBar / Menu / MenuItem

Only valid as a desktop child (`singleWindow: false`). Items support `shortcut`,
`enabled`, nested submenus (via nested `MenuItem` children), and `onClick`.

```tsx
<MenuBar>
  <Menu title="File">
    <MenuItem label="New" shortcut="Ctrl+N" onClick={newDoc} />
    <MenuItem label="Open" shortcut="Ctrl+O" onClick={openDoc} />
  </Menu>
  <Menu title="Help">
    <MenuItem label="About" onClick={showAbout} />
  </Menu>
</MenuBar>
```

## StatusBar

A fixed desktop slot with `left` and `right` text. It takes no children.

```tsx
<StatusBar left="Ready" right={`Ln ${line}, Col ${col}`} />
```

## Event payloads

Every handler can take an `AttoUiCallbackEvent` as its last argument when you
need more than the convenience value:

```tsx
<TextBox value={v} onChange={(value, event) => {
  console.log(event.targetId, event.payload, event.nativeEvent)
  setV(value)
}} />
```

## Raw host intrinsics (advanced)

Beyond the wrappers above, the reconciler exposes lowercase JSX intrinsics for
runtime components that don't yet have a typed wrapper — e.g. `checkbox`,
`radioGroup`, `slider`, `progressBar`, `spinner`. These take runtime-shaped
props (often `snake_case`) and an `onChange` callback handle:

```tsx
<checkbox label="Enable" checked={on} onChange={handle} />
<slider min={0} max={100} value={v} onChange={handle} />
<progressBar min={0} max={100} value={pct} show_text text={`${pct}%`} />
```

Prefer the capitalized wrappers when one exists; reach for raw intrinsics only
for components without a wrapper.
