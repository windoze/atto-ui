# @atto-ui/react

A [React reconciler](https://github.com/facebook/react/tree/main/packages/react-reconciler)
and typed JSX components for [atto-ui](../../README.md). Write terminal UIs with
the same component model, hooks, and state flow you already use on the web — the
reconciler commits your tree to the native atto-ui runtime instead of the DOM.

```tsx
import { useState } from 'react'
import { Button, Text, VStack, Window, render } from '@atto-ui/react'

function Counter() {
  const [count, setCount] = useState(0)
  return (
    <Window title="Counter" rect={[2, 1, 36, 8]}>
      <VStack spacing={1} padding={1}>
        <Text>{`Count: ${count}`}</Text>
        <Button onClick={() => setCount((n) => n + 1)}>Increment</Button>
      </VStack>
    </Window>
  )
}

render(<Counter />, { singleWindow: false })
```

## Install

```sh
npm install @atto-ui/react react
```

`react` (18.x) and the native binary from `@atto-ui/core` are required. A single
React copy must be shared with the reconciler — two copies cause an
"Invalid hook call" error.

## Rendering

`render(element, options)` mounts a tree and starts a non-blocking tick loop.

| Option | Default | Notes |
|---|---|---|
| `singleWindow` | `true` | When `true`, wraps the tree in one full-screen `Window`. Set `false` to manage `Window`/`MenuBar`/`StatusBar` yourself. |
| `headless` | `false` | Use an in-memory terminal (tests/CI). |
| `cols` / `rows` | terminal size | Headless dimensions. |
| `idPrefix` | auto | Prefix for generated component ids. |

`render` returns a handle with `host` (the underlying `AppHost`), `windowIds()`,
and `stop()`.

## Components

| Export | Host element | Purpose |
|---|---|---|
| `Window` | `window` | A desktop window; requires `rect={[x, y, w, h]}`. Reports `onClose`/`onMinimize`/`onMaximize`/`onRestore`. |
| `VStack` / `HStack` | `vstack` / `hstack` | Vertical / horizontal layout. `spacing`, `padding`, `scrollable`. |
| `Grid` | `grid` | Column grid with `columns`, `rowGap`, `columnGap`. |
| `Border` | `border` | Bordered container around children. |
| `Divider` | `divider` | Horizontal/vertical rule (`orientation`). |
| `Disclosure` | `disclosure` | Collapsible section: `expanded` + `onToggle`. |
| `Button` | `button` | `onClick`; label from children or `label`. |
| `Label` | `label` | Static single-line text (`text`, `enabled`). |
| `TextBox` | `textBox` | Controlled single-line input: `value` + `onChange(value)`. |
| `TextArea` | `textArea` | Controlled multi-line input: `value`, `height`, `enterSubmits`. |
| `Checkbox` | `checkbox` | Controlled boolean: `checked` + `onChange(checked)`. |
| `RadioGroup` | `radioGroup` | `options`, `selectedIndex`, `onChange(index)`. |
| `Slider` | `slider` | Numeric `value` in `[min, max]`, `onChange(value)`. |
| `ProgressBar` | `progressBar` | Read-only `value`; optional `showText`/`text`. |
| `Spinner` | `spinner` | Activity indicator: `running`, `text`. |
| `ListBox` | `listBox` | `items`, `selectedIndex`, `onSelect(index)`. |
| `Table` / `TableView` | `tableView` | `headers`, `rows`, `onSelect(index)`. |
| `FileTree` | `fileTree` | `nodes`, `selection`, `onSelect`/`onRename`/`onDelete`, `icons`. |
| `Editor` | `editor` | Code editor: `value`, `languageId`, `showLineNumbers`, `readOnly`, … |
| `Text` + `B`/`I`/`U`/`S`/`Link` | `richText` / `textSpan` | Inline styled text. |
| `Markdown` | `markdownViewer` | Block markdown rendering. |
| `Desktop` | desktop root | Explicit desktop root (usually implicit with `singleWindow: false`). |
| `MenuBar` / `Menu` / `MenuItem` | menu slots | Desktop menu bar. |
| `MinimizedWindowsMenu` | menu slot | Runtime-filled list of minimized windows; restores on click. No `onClick`/children. |
| `WindowOpMenuItem` | menu slot | Menu item bound to a built-in window op (`cascade`, `tile`, `close`, …). |
| `StatusBar` | `statusBar` | Desktop status bar (`left` / `right`). |

Lowercase host elements (e.g. `<vstack>`, `<textBox>`) are also available as
typed JSX intrinsics for advanced use; the capitalized wrappers above are the
recommended surface.

## Tips

- `<Text>{`Count: ${n}`}</Text>` renders as a single span;
  `<Text>Count: {n}</Text>` splits into separate spans.
- `TextBox`/`ListBox`/`Table` are controlled — store their value in React state
  and update it from the change handler.
- Event handlers receive an `AttoUiCallbackEvent` as the last argument if you
  need `targetId`, the raw `payload`, or `nativeEvent`.

## Examples

Runnable TSX demos live in [`examples/react-tsx`](../../examples/react-tsx):
hello, counter, todo list, multi-window desktop, streaming markdown, runtime
theme switch, and a component gallery covering every wrapper above.

## Docs

- Getting started: [`docs/REACT_GETTING_STARTED.md`](../../docs/REACT_GETTING_STARTED.md)
- API reference: [`docs/NODE_API.md`](../../docs/NODE_API.md)
- Underlying facade: [`@atto-ui/core`](../core)
