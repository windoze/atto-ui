# Node And React API

This document covers the JavaScript-facing API provided by `@atto-ui/node`, `@atto-ui/core`, and `@atto-ui/react`.

> New to the React layer? Start with the [Getting Started tutorial](./REACT_GETTING_STARTED.md)
> and the runnable [TSX demos](../examples/react-tsx). Per-package READMEs:
> [`@atto-ui/core`](../packages/core/README.md), [`@atto-ui/react`](../packages/react/README.md).

## Package Layout

| Package | Runtime role |
|---|---|
| `@atto-ui/node` | Raw N-API package generated from `crates/atto-ui-node`. |
| `@atto-ui/node-darwin-arm64` | macOS arm64 native binary package. |
| `@atto-ui/node-darwin-x64` | macOS x64 native binary package. |
| `@atto-ui/node-linux-x64-gnu` | Linux x64 glibc native binary package. |
| `@atto-ui/node-win32-x64-msvc` | Windows x64 MSVC native binary package. |
| `@atto-ui/core` | Typed CommonJS facade and spec builders. |
| `@atto-ui/react` | React reconciler, JSX components, and render loop. |

`@atto-ui/core` loads native bindings in this order:

1. `ATTO_UI_NATIVE_LIBRARY_PATH`.
2. `NAPI_RS_NATIVE_LIBRARY_PATH`.
3. A local `atto_ui_node.<platform>.node` next to `@atto-ui/core`.
4. `@atto-ui/core-<platform>` or `@atto-ui/node-<platform>` optional packages.
5. `@atto-ui/node`.
6. The workspace fallback at `crates/atto-ui-node` for local development.

## `AppHost`

`AppHost` owns the terminal session, desktop, windows, runtime component tree, and callback queue.

```ts
new AppHost(config?: AppHostConfig | null)
```

`AppHostConfig`:

| Field | Type | Default | Notes |
|---|---|---|---|
| `headless` | `boolean` | `false` | Use an in-memory terminal for tests. |
| `cols` / `rows` | `number` | terminal size or `80x24` headless | Headless dimensions. |
| `tickRate` | `number` | `0` | Milliseconds passed to crossterm polling. `0` is non-blocking. |
| `mouseCapture` | `boolean` | `true` | Enables mouse capture in real-terminal mode. |
| `hideCursor` | `boolean` | `true` | Hides cursor while active. |
| `bracketedPaste` | `boolean` | `false` | Enables bracketed paste in real-terminal mode. |
| `keyboardEnhancement` | `boolean` | `true` | Enables crossterm keyboard enhancement flags when available. |

Primary methods:

| Method | Notes |
|---|---|
| `addDynamicWindow(title, rect, root)` | Adds a runtime window and returns an opaque string window handle. |
| `applyTreeOps(windowId, opOrOps)` | Applies one or more `TreeOp` values to one window. |
| `step()` | Advances one non-blocking frame; returns `false` when the host requests exit. |
| `dispose()` | Restores terminal state; idempotent and safe for headless hosts. |
| `drainCallbacks()` | Returns queued UI callback invocations. |
| `allocCallback()` / `releaseCallback(id)` | Manages opaque string callback handles. |
| `sendEvent(windowId, event)` | Injects a key, mouse, paste, resize, or focus event into one window. |
| `closeWindow` / `focusWindow` / `moveWindow` / `resizeWindow` / `setTitle` | Window management. |
| `setMenuBar(spec)` / `setStatusBar(left, right)` | Desktop chrome slots used by React desktop roots. |
| `setProperty(id, name, value)` / `getProperty(id, name)` | Runtime property access by component id. |
| `snapshot()` | Deterministic desktop snapshot for tests. |
| `schemas()` | Registered component schema metadata. |

## Component Specs

Runtime UI is described by plain objects:

```ts
type ComponentSpec = {
  type: string
  id?: string
  props?: Record<string, ComponentValue>
  events?: Record<string, string>
  children?: ComponentSpecChild[]
}
```

`ComponentValue` accepts `null`, booleans, numbers, strings, string lists, string tables, byte arrays, rects, lists, maps, and tagged escape forms such as `{ $type: 'rect', data: [x, y, width, height] }`.

Important `TreeOp` variants:

| `op` | Purpose |
|---|---|
| `set_tree` | Replace a window root tree. |
| `insert_before` | Insert or move a child before an anchor id; `anchor_id: null` appends. |
| `remove` / `replace` / `move` | Structural edits. |
| `set_prop` / `clear_prop` | Property updates and default restoration. |
| `bind_event` / `clear_event` | Event callback handle binding. |

Callback invocations have this shape:

```ts
type CallbackInvocation = {
  callbackId: string
  targetId: string | null
  event: string
  payload: ComponentValue | null
}
```

Callback and window ids are opaque string handles. JavaScript code must not parse them or perform arithmetic on them.

## Core Builders

`@atto-ui/core` exports thin constructors that return plain `ComponentSpec` objects:

```js
const { Button, Text, VStack } = require('@atto-ui/core')

const root = VStack({ id: 'root', spacing: 1 }, [
  Text('Ready', { id: 'title' }),
  Button({ id: 'submit', text: 'Submit', onClick: 'atto:callback:1' }),
])
```

Builder props accept camelCase convenience names where the runtime uses snake_case. Event aliases such as `onClick`, `onChange`, `onSelect`, `onSubmit`, and `onLink` are converted into `events` entries with callback handles.

## React API

`@atto-ui/react` exposes a React reconciler and component wrappers.

```ts
render(element, options?): RenderHandle
```

`RenderOptions`:

| Field | Notes |
|---|---|
| `singleWindow` | Defaults to `true`; wraps the app in a full-screen `Window`. Use `false` for explicit multi-window trees. |
| `headless` | Uses an in-memory terminal for tests. |
| `cols` / `rows` | Terminal size. |
| `idPrefix` | Stable id prefix for deterministic tests. |

`RenderHandle` exposes `host`, `root`, `windowId`, `windowIds()`, and `stop()`.

Common wrappers:

| Component | Runtime mapping |
|---|---|
| `Button` | `Button`, `onClick -> click`. |
| `TextBox` | Controlled text input, `value` and `onChange(value, event)`. |
| `ListBox` / `Table` | Selection payloads as numbers. |
| `VStack` / `HStack` / `Grid` | Layout containers. |
| `Text`, `B`, `I`, `U`, `S`, `Link` | Structured `RichText` + `TextSpan`. |
| `Markdown` | `MarkdownViewer`. |
| `Desktop`, `Window`, `MenuBar`, `Menu`, `MenuItem`, `StatusBar` | Virtual desktop root and chrome mapping. |

Events are not called directly from Rust. The binding queues callback invocations, the React render loop drains them after each `step()`, and handlers can safely call `setState`.

## Runtime Compatibility

The native binding is N-API based and is validated on Node, Bun, and Deno.

```sh
npm run test:runtime:node --prefix packages/core
npm run test:runtime:bun --prefix packages/core
npm run test:runtime:deno --prefix packages/core
```

Deno requires explicit native and process permissions. The package script uses:

```sh
deno run --allow-read --allow-env --allow-run --allow-ffi __test__/runtime_compat.cjs
```

The compatibility suite includes:

- Headless N-API load, `AppHost` construction, `applyTreeOps`, event injection, and callback drain.
- PTY raw-mode startup and Ctrl+Q shutdown on POSIX platforms; the PTY smoke is skipped on Windows.
- Restoration assertions for alternate screen, cursor visibility, mouse capture, and terminal raw-mode flags.

Known behavior:

- Deno must be launched with `--allow-ffi` for native `.node` loading; the local loader and PTY smoke also require `--allow-read --allow-env --allow-run`.
- Real-terminal apps should avoid `console.log` while the alternate screen is active; render logs inside the UI or write them to a file.
- `AppHost` is single-threaded and should stay on the main runtime thread. Do not move it across workers.
