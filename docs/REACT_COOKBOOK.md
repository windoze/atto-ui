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

## TextArea (controlled)

Multi-line input. Like `TextBox` but with a `height` and an `enterSubmits` knob
(when `false`, Enter inserts a newline and `onSubmit` is not fired).

```tsx
const [body, setBody] = useState('')

<TextArea
  title="Message"
  height={6}
  value={body}
  enterSubmits={false}
  onChange={(value) => setBody(value)}
/>
```

## Label

A static single-line text widget (distinct from rich `Text`). Use it for plain
captions next to other widgets.

```tsx
<Label text="Project files" />
<Label text="Disabled" enabled={false} />
```

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

## Checkbox

Controlled boolean. `onChange` receives the next `checked` value.

```tsx
const [on, setOn] = useState(false)

<Checkbox label="Enable feature" checked={on} onChange={setOn} />
```

## RadioGroup

Controlled single choice over `options`. `onChange` receives the selected index.

```tsx
const [choice, setChoice] = useState(0)

<RadioGroup
  label="Mode"
  options={['Auto', 'Manual', 'Off']}
  selectedIndex={choice}
  onChange={setChoice}
/>
```

## Slider

Numeric input in `[min, max]` with an optional `step`. `onChange` receives the
new value.

```tsx
const [volume, setVolume] = useState(50)

<Slider min={0} max={100} step={5} value={volume} onChange={setVolume} />
```

## ProgressBar

Read-only progress indicator. Set `showText` to render a percentage, or pass an
explicit `text`.

```tsx
<ProgressBar min={0} max={100} value={pct} showText />
<ProgressBar min={0} max={100} value={pct} text={`${pct}%`} />
```

## Spinner

Activity indicator. Set `running` to animate; `text` adds a trailing label.

```tsx
<Spinner running text="Loading…" />
```

## Disclosure

A collapsible section. Controlled via `expanded` + `onToggle`; children render
when expanded.

```tsx
const [open, setOpen] = useState(true)

<Disclosure title="Details" expanded={open} onToggle={setOpen}>
  <Text>Hidden until expanded.</Text>
</Disclosure>
```

## Divider

A horizontal (default) or vertical rule.

```tsx
<Divider />
<Divider orientation="vertical" />
```

## Border

Wraps children in a box border.

```tsx
<Border>
  <VStack padding={1}>
    <Text>Boxed content</Text>
  </VStack>
</Border>
```

## Editor

A full code editor widget. Common knobs: `languageId`, `showLineNumbers`,
`showFoldingMarkers`, `readOnly`, `tabWidth`, `insertSpaces`.

```tsx
<Editor
  value={"fn main() {\n    println!(\"hi\");\n}"}
  languageId="rust"
  showLineNumbers
  layout={{ height: 'fill' }}
/>
```

## FileTree

A file/directory tree. `nodes` is a list of `{ id, name, kind?, expanded?,
children? }`; `selection` is the controlled selected id; `onSelect` reports the
new id (or `null` when cleared). `icons` maps file extensions to glyphs/colors.

```tsx
const NODES = [
  {
    id: 1,
    name: 'src',
    kind: 'directory',
    expanded: true,
    children: [
      { id: 2, name: 'main.rs' },
      { id: 3, name: 'lib.rs' },
    ],
  },
  { id: 4, name: 'README.md' },
] as const

const ICONS = {
  rs: { glyph: 'rs', color: '#dd6644' },
  md: { glyph: 'md', color: '#66aadd' },
} as const

const [selected, setSelected] = useState<number | null>(null)

<FileTree
  nodes={NODES}
  icons={ICONS}
  selection={selected}
  onSelect={(id) => setSelected(id)}
  onRename={(payload) => rename(payload)}
  onDelete={(payload) => remove(payload)}
/>
```

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

For window management, use the built-in items instead of wiring your own
handlers: `<MinimizedWindowsMenu />` (a runtime-filled submenu of minimized
windows) and `<WindowOpMenuItem op="cascade" />` for operations like `cascade`,
`tile`, `minimize`, `maximize`, `restore`, `close`, `next`, `previous`,
`minimizeAll`, `restoreAll`, `closeAll`. The desktop owns these actions.

```tsx
<Menu title="Window">
  <WindowOpMenuItem op="cascade" />
  <WindowOpMenuItem op="tile" />
  <WindowOpMenuItem op="closeAll" label="Close all" />
  <MinimizedWindowsMenu />
</Menu>
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

## Chat (agent transcript)

`ChatMessageList` renders the block-based chat model (a message is an ordered
list of blocks: text / thinking / tool_use / tool_result / diff / plan / todo /
task / notice / attachment / artifact). `ChatInputPanel` is the input box, and
`ChatPanel` stacks the two. `useChatMessages` manages the transcript in React
state with helpers that mirror the Rust `ChatMessageStore` (streaming, tool
results, decisions), so you never touch a native store.

```tsx
import {
  ChatPanel,
  useChatMessages,
} from '@atto-ui/react'

function Chat() {
  const chat = useChatMessages()

  function send(event) {
    const payload = event.payload as { kind?: string; text?: string } | null
    chat.push({
      id: chat.nextMessageId(),
      role: 'user',
      status: 'complete',
      blocks: [{ type: 'text', block_id: chat.nextBlockId(), markdown: payload?.text ?? '' }],
    })
    streamReply(chat, payload?.text ?? '')
  }

  return (
    <ChatPanel
      list={{
        messages: chat.messages,
        // bubbles span 75% of the width by default; fillWidth (= bubbleWidthPercent 100)
        // makes assistant/user messages use the full list width.
        fillWidth: true,
        // every runtime event is wired through camelCase props:
        onApprove: (e) => console.log('approve', e.payload),
        onEditDecision: (e) => console.log('diff decision', e.payload),
        onPlanDecision: (e) => console.log('plan decision', e.payload),
        onCancel: (e) => console.log('cancelled', e.payload),
        onMessageAction: (e) => console.log('action', e.payload),
        onOpenArtifact: (e) => console.log('open artifact', e.payload),
        onLoadMore: () => loadOlder(chat),
      }}
      input={{ mode: { kind: 'text', title: 'Message' }, clearOnSubmit: true, onSubmit: send }}
      spacing={1}
    />
  )
}

// Stream an assistant turn block-by-block.
function streamReply(chat, prompt) {
  const { messageId, blockId } = chat.addTextTurn('assistant', '', { status: 'streaming' })
  let i = 0
  const text = `You said: ${prompt}`
  const timer = setInterval(() => {
    if (i < text.length) chat.appendTextDelta(blockId, text[i++])
    else { chat.setTurnStatus(messageId, 'complete'); clearInterval(timer) }
  }, 40)
}
```

The input panel supports three modes via the friendly `mode` descriptor:

```tsx
<ChatInputPanel mode={{ kind: 'text', title: 'Message' }} onSubmit={send} />
<ChatInputPanel mode={{ kind: 'choice', title: 'Pick', options: ['Yes', 'No'], allowCustom: true }} onSubmit={send} />
<ChatInputPanel mode={{ kind: 'confirm', prompt: 'Run command?', yesLabel: 'Run', noLabel: 'Skip' }} onSubmit={send} />
```

`onSubmit` payload is a map: `{ kind: 'text', text }`, `{ kind: 'choice', index, label }`,
or `{ kind: 'custom', text }`. The list event payloads (`onApprove`,
`onEditDecision`, `onPlanDecision`, `onCancel`, `onMessageAction`) are maps
carrying `message_id` / `block_id` and the relevant decision/action; apply them
back to the transcript with the matching `useChatMessages` helper
(`resolveApproval`, `setEditDecision`, `setPlanDecision`, `setTurnStatus`, …).

## Raw host intrinsics (advanced)

Every capitalized wrapper has a matching lowercase JSX intrinsic (e.g.
`<vstack>`, `<textBox>`, `<checkbox>`, `<slider>`, `<progressBar>`). The
intrinsics take runtime-shaped props (often `snake_case`) and raw `onChange`
callback handles instead of the convenience value handlers:

```tsx
<checkbox label="Enable" checked={on} onChange={handle} />
<slider min={0} max={100} value={v} onChange={handle} />
<progressBar min={0} max={100} value={pct} show_text text={`${pct}%`} />
```

Prefer the capitalized wrappers — they add controlled-value ergonomics and
typed props. Reach for raw intrinsics only when a runtime component has no
wrapper yet, or when you need a prop the wrapper does not expose.
