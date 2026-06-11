# Getting Started with @atto-ui/react

This is a step-by-step tutorial for building terminal UIs with React and
atto-ui. Every snippet is real TSX you can run; the finished versions live in
[`examples/react-tsx`](../examples/react-tsx).

If you prefer a reference over a tutorial, see [`NODE_API.md`](./NODE_API.md)
and the [`@atto-ui/react` README](../packages/react/README.md).

## Mental model

`@atto-ui/react` is a React reconciler — the same idea as `react-dom`, but it
commits your component tree to the native atto-ui runtime instead of the DOM.

- You write components, hooks, and state exactly as on the web.
- `render()` mounts the tree and starts a **non-blocking tick loop** that polls
  the terminal, applies React updates, and dispatches input events back to your
  handlers.
- Layout, drawing, focus, and Unicode handling all happen in the Rust core.

```
React components ──▶ reconciler ──▶ AppHost (native) ──▶ terminal
        ▲                                  │
        └────────── input events ◀─────────┘
```

## 0. Setup

From the repository root, build the native binding and the React package once
(see [`examples/react-tsx/README.md`](../examples/react-tsx/README.md) for the
exact commands), then in your own project:

```sh
npm install @atto-ui/react react
```

A single React copy must be shared with the reconciler. Two copies (a common
monorepo/`file:` pitfall) produce `Invalid hook call`.

## 1. Hello window

The smallest app: one `Window` containing a layout and some text.

```tsx
import { Text, VStack, Window, render } from '@atto-ui/react'

function App() {
  return (
    <Window title="Hello" rect={[2, 1, 40, 8]}>
      <VStack spacing={1} padding={1}>
        <Text>Welcome to atto-ui + React.</Text>
        <Text>Press Ctrl+Q to quit.</Text>
      </VStack>
    </Window>
  )
}

render(<App />, { singleWindow: false })
```

`rect` is `[x, y, width, height]` in terminal cells. `singleWindow: false` means
"I will provide my own windows"; the default `true` wraps your tree in one
full-screen window.

## 2. State and events

Hooks work as usual. Wire a `Button`'s `onClick` to a state setter.

```tsx
import { useState } from 'react'
import { Button, Text, VStack, Window, render } from '@atto-ui/react'

function App() {
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

render(<App />, { singleWindow: false })
```

> **Text interpolation:** `<Text>{`Count: ${count}`}</Text>` renders as one span.
> `<Text>Count: {count}</Text>` produces two spans ("Count: " and the number),
> which matters if you assert on snapshot text.

## 3. Controlled inputs

`TextBox` is controlled like a React form input: pass `value`, update it from
`onChange`. `ListBox` reports its selection through `onSelect`.

```tsx
import { useState } from 'react'
import { Button, ListBox, TextBox, VStack, Window, render } from '@atto-ui/react'

function App() {
  const [draft, setDraft] = useState('')
  const [items, setItems] = useState<string[]>([])
  const [selected, setSelected] = useState(0)

  function add() {
    if (!draft.trim()) return
    setItems([...items, draft.trim()])
    setSelected(items.length)
    setDraft('')
  }

  return (
    <Window title="Todos" rect={[2, 1, 44, 14]}>
      <VStack spacing={1} padding={1}>
        <TextBox title="New todo" value={draft} onChange={setDraft} />
        <Button onClick={add}>Add</Button>
        <ListBox title="Items" height={6} items={items} selectedIndex={selected} onSelect={setSelected} />
      </VStack>
    </Window>
  )
}

render(<App />, { singleWindow: false })
```

The native widget's current text lives in the runtime tree's `properties.text`,
not as a visible label. To display it elsewhere, mirror the state into a
`<Text>` node.

## 4. A full desktop: windows, menu, status bar

With `singleWindow: false` the root accepts multiple `Window`s plus the
`MenuBar` and `StatusBar` slots. Menu items use the same `onClick` model as
buttons.

```tsx
import { useState } from 'react'
import { Button, Menu, MenuBar, MenuItem, StatusBar, Text, VStack, Window, render } from '@atto-ui/react'

function App() {
  const [log, setLog] = useState('ready')
  return (
    <>
      <MenuBar>
        <Menu title="File">
          <MenuItem label="New" shortcut="Ctrl+N" onClick={() => setLog('New')} />
        </Menu>
      </MenuBar>
      <Window title="Main" rect={[1, 1, 34, 9]}>
        <VStack padding={1}>
          <Button onClick={() => setLog('Ping')}>Ping</Button>
        </VStack>
      </Window>
      <Window title="Activity" rect={[37, 1, 34, 9]}>
        <VStack padding={1}>
          <Text>{`Last action: ${log}`}</Text>
        </VStack>
      </Window>
      <StatusBar left="atto-ui" right={log} />
    </>
  )
}

render(<App />, { singleWindow: false })
```

## 5. Streaming (LLM-style)

Because the tick loop is non-blocking, async work keeps the UI responsive. Feed
a `Markdown` viewer from a `for await` stream — exactly how you'd pipe tokens
from an LLM SDK.

```tsx
import { useEffect, useState } from 'react'
import { Markdown, Window, render } from '@atto-ui/react'

async function* tokens() {
  for (const t of ['# Reply\n\n', 'Streaming ', 'token ', 'by token.']) {
    await new Promise((r) => setTimeout(r, 60))
    yield t
  }
}

function App() {
  const [text, setText] = useState('')
  useEffect(() => {
    let cancelled = false
    void (async () => {
      for await (const token of tokens()) {
        if (cancelled) return
        setText((current) => current + token)
      }
    })()
    return () => { cancelled = true }
  }, [])
  return (
    <Window title="Assistant" rect={[2, 1, 50, 14]}>
      <Markdown markdown={text} />
    </Window>
  )
}

render(<App />, { singleWindow: false })
```

For a complete agent example with a real OpenAI/Anthropic provider, see
[`examples/node/agent_chat.cjs`](../examples/node/agent_chat.cjs)
([README](../examples/node/README.md)).

## 6. Routing (React Router)

`@atto-ui/react` is a DOM-free reconciler, so React Router works — but only
through its **`MemoryRouter`**. `BrowserRouter` / `HashRouter` and the DOM
`<Link>` / `<NavLink>` rely on `window.location` and `<a>` elements that don't
exist in a terminal. Install the core `react-router` package, keep the history
in memory, and navigate from atto-ui `Button` / `MenuItem` handlers with
`useNavigate()`.

```sh
npm install react-router
```

```tsx
import { MemoryRouter, Route, Routes, useLocation, useNavigate } from 'react-router'
import { Button, Divider, Text, VStack, Window, render } from '@atto-ui/react'

function Home() {
  const navigate = useNavigate()
  return (
    <VStack spacing={1} padding={1}>
      <Text>Home — pick a destination.</Text>
      <Button onClick={() => navigate('/about')}>Open About</Button>
    </VStack>
  )
}

function About() {
  const navigate = useNavigate()
  return (
    <VStack spacing={1} padding={1}>
      <Text>About page.</Text>
      <Button onClick={() => navigate(-1)}>Back</Button>
    </VStack>
  )
}

function App() {
  return (
    <Window title="Router" rect={[2, 1, 46, 12]}>
      <MemoryRouter>
        <VStack padding={1}>
          <Breadcrumb />
          <Divider />
          <Routes>
            <Route path="/" element={<Home />} />
            <Route path="/about" element={<About />} />
          </Routes>
        </VStack>
      </MemoryRouter>
    </Window>
  )
}

function Breadcrumb() {
  const { pathname } = useLocation()
  return <Text>{`Path: ${pathname}`}</Text>
}

render(<App />, { singleWindow: false })
```

Notes:

- Use `react-router` (core), not `react-router-dom`'s `BrowserRouter`. The data
  router (`createMemoryRouter` + `<RouterProvider>`) works too.
- Route `element`s render atto-ui components, never DOM tags.
- The single-React-copy rule applies: `react-router` must resolve to the same
  `react` as the reconciler, or you'll hit `Invalid hook call`.
- Runnable version: [`examples/react-tsx/src/08-router.tsx`](../examples/react-tsx/src/08-router.tsx).

## 7. Forms with React Hook Form

React Hook Form's state and validation are pure logic, so they work — but its
default `register()` returns a DOM input `ref` that has nothing to attach to in
a terminal. Use **`Controller`** (or `useController`) instead, the same path RHF
uses for React Native and UI libraries: it gives you `field.value` /
`field.onChange` to wire onto atto-ui's controlled widgets.

```sh
npm install react-hook-form
```

```tsx
import { Controller, useForm } from 'react-hook-form'
import { Button, Text, TextBox, VStack, Window, render } from '@atto-ui/react'

type SignUp = { name: string; email: string }

function App() {
  const { control, handleSubmit, formState: { errors } } = useForm<SignUp>({
    mode: 'onChange',
    defaultValues: { name: '', email: '' },
  })

  const onValid = (data: SignUp) => { /* submit data */ }

  return (
    <Window title="Sign up" rect={[2, 1, 48, 14]}>
      <VStack spacing={1} padding={1}>
        <Controller
          name="email"
          control={control}
          rules={{
            required: 'Email is required',
            pattern: { value: /^[^@\s]+@[^@\s]+$/, message: 'Invalid email' },
          }}
          render={({ field }) => (
            <TextBox title="Email" value={field.value} onChange={field.onChange} />
          )}
        />
        {errors.email && <Text>{`! ${errors.email.message}`}</Text>}

        <Button onClick={() => void handleSubmit(onValid)()}>Submit</Button>
      </VStack>
    </Window>
  )
}

render(<App />, { singleWindow: false })
```

Notes:

- Use `Controller` / `useController`, **not `register()`** (it needs a DOM ref).
- atto-ui widgets pass the new value as the first `onChange` argument, which is
  exactly what `field.onChange` expects.
- Don't forward `field.ref` to atto-ui components — they take no DOM ref, so
  RHF's auto-focus-first-error is unavailable, but validation is unaffected.
- `Button` expects a `() => void` handler, so call the submit handler inside an
  arrow: `onClick={() => void handleSubmit(onValid)()}`.
- Rules (`required` / `pattern` / `validate`) and zod/yup resolvers all work.
- Runnable version: [`examples/react-tsx/src/09-form-validation.tsx`](../examples/react-tsx/src/09-form-validation.tsx).

## Testing without a terminal

`render(element, { headless: true, cols, rows })` runs against an in-memory
terminal. You can then inspect `handle.host.snapshot()` and drive synthetic
input with `handle.host.sendEvent(windowId, { type: 'key', key: 'enter' })`.
The TSX demos use this for deterministic smoke checks (`npm run smoke`).

## Next steps

- [Component cookbook](./REACT_COOKBOOK.md) — focused snippets per component.
- [API reference](./NODE_API.md) — `AppHost` methods and full type shapes.
- [TSX demos](../examples/react-tsx) — runnable versions of everything above.
