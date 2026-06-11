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
| `@atto-ui/node-linux-x64-gnu` | Linux x64 glibc native binary package. |
| `@atto-ui/node-win32-x64-msvc` | Windows x64 MSVC native binary package. |
| `@atto-ui/core` | Typed CommonJS facade and spec builders. |
| `@atto-ui/react` | React reconciler, JSX components, and render loop. |

Published native binary packages currently cover macOS arm64, Linux x64 (glibc),
and Windows x64 (MSVC). macOS x64 is not published; build it locally from
`crates/atto-ui-node` if you need it.

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
| `drainWindowEvents()` | Returns queued window lifecycle events (close/minimize/maximize/restore). |
| `allocCallback()` / `releaseCallback(id)` | Manages opaque string callback handles. |
| `sendEvent(windowId, event)` | Injects a key, mouse, paste, resize, or focus event into one window. |
| `closeWindow` / `focusWindow` / `moveWindow` / `resizeWindow` / `setTitle` | Per-window management. |
| `minimizeWindow` / `maximizeWindow` / `restoreWindow` | Window state changes by id. |
| `listWindows()` | Current window handles and their state. |
| `cascadeWindows()` / `tileWindows()` | Arrange all open windows. |
| `focusNextWindow()` / `focusPreviousWindow()` | Cycle focus across windows. |
| `minimizeAllWindows()` / `restoreAllWindows()` / `closeAllWindows()` | Bulk window operations. |
| `setMenuBar(spec)` / `setStatusBar(left, right)` | Desktop chrome slots used by React desktop roots. |
| `setProperty(id, name, value)` / `getProperty(id, name)` | Runtime property access by component id. |
| `setTheme(name)` | Switch to a built-in theme by name (e.g. `dark`, `light`). |
| `loadTheme(path, base?)` | Load a JSON/YAML theme file, optionally extending a built-in `base`. |
| `snapshot()` | Deterministic desktop snapshot for tests. |
| `schemas()` | Registered component schema metadata. |

The N-API package also exports two module-level functions:

| Function | Notes |
|---|---|
| `registerAllRuntimeComponents()` | Registers optional runtime components from workspace companion crates. `@atto-ui/core` calls this on load; call it manually only when using the raw `@atto-ui/node` package directly. |
| `version()` | Returns the native package version string (used by smoke tests). |

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
| `Label` | Static single-line text (`text`, `enabled`). |
| `TextBox` | Controlled single-line input, `value` and `onChange(value, event)`. |
| `TextArea` | Controlled multi-line input, `value` / `onChange`, optional `enterSubmits`. |
| `ListBox` / `Table` / `TableView` | Selection payloads as numbers (`onSelect` / `onChange`). |
| `Checkbox` | Controlled boolean, `checked` + `onChange(checked)`. |
| `RadioGroup` | `options`, controlled `selectedIndex` + `onChange(index)`. |
| `Slider` | Numeric `value` in `[min, max]`, `onChange(value)`. |
| `ProgressBar` | Read-only numeric `value` in `[min, max]`, optional `showText`/`text`. |
| `Spinner` | Activity indicator, `running` + optional `text`. |
| `Disclosure` | Collapsible section, controlled `expanded` + `onToggle(expanded)`. |
| `Divider` | Horizontal/vertical rule (`orientation`). |
| `Border` | Bordered container around `children`. |
| `Editor` | Code editor (`value`, `languageId`, `showLineNumbers`, `showFoldingMarkers`, `readOnly`, `tabWidth`, `insertSpaces`). |
| `FileTree` | Tree of `nodes`, controlled `selection`, `onSelect`/`onRename`/`onDelete`, optional `icons`. |
| `VStack` / `HStack` / `Grid` | Layout containers. |
| `Text`, `B`, `I`, `U`, `S`, `Link` | Structured `RichText` + `TextSpan`. |
| `Markdown` | `MarkdownViewer`. |
| `Desktop`, `Window`, `MenuBar`, `Menu`, `MenuItem`, `StatusBar` | Virtual desktop root and chrome mapping. |
| `MinimizedWindowsMenu` | Runtime-managed list of minimized windows (see below). |
| `WindowOpMenuItem` | Menu item wired to a built-in window operation (see below). |

`Window` reports lifecycle through `onClose` / `onMinimize` / `onMaximize` /
`onRestore` (drained from `drainWindowEvents()`); controlled widgets
(`TextBox`, `TextArea`, `ListBox`, `Table`, `Checkbox`, `RadioGroup`, `Slider`,
`FileTree`) keep their value in React state and update it from the change
handler.

Events are not called directly from Rust. The binding queues callback invocations, the React render loop drains them after each `step()`, and handlers can safely call `setState`.

### Reserved menu ids

The native runtime recognizes one reserved menu item id:

| Id | Behavior |
|---|---|
| `atto_ui:minimized_windows` | The desktop refills this item's submenu every frame with the currently minimized windows and restores the chosen window when an entry is selected. |

This is a plain spec-level convention, so it works from any layer:

- **`@atto-ui/react`**: use `<MinimizedWindowsMenu />` (optionally `label="..."`), or a bare `<MenuItem id="atto_ui:minimized_windows" label="..." />`. The exported constant `MINIMIZED_WINDOWS_MENU_ID` holds the id.
- **Raw `setMenuBar` spec / `@atto-ui/core`**: add a `MenuItemSpec` whose `id` equals `atto_ui:minimized_windows` with an empty `items` array.

```tsx
import { Menu, MenuItem, MinimizedWindowsMenu } from '@atto-ui/react'

<Menu title="Window">
  <MenuItem label="New" shortcut="Ctrl+N" onClick={onNew} />
  <MinimizedWindowsMenu />
</Menu>
```

Do not provide your own `items`/children or an `onClick` for this item — the submenu and the restore action are owned by Rust, and any children you set are overwritten each frame.

### Window operation menu items

The runtime also recognizes a family of reserved menu item ids (`atto_ui:window_*`)
that perform built-in window operations without any JavaScript handler. The
desktop owns the action, so you only supply the menu item.

| Operation | Reserved id |
|---|---|
| `cascade` | `atto_ui:window_cascade` |
| `tile` | `atto_ui:window_tile` |
| `minimize` / `maximize` / `restore` | `atto_ui:window_minimize` / `_maximize` / `_restore` |
| `close` | `atto_ui:window_close` |
| `next` / `previous` | `atto_ui:window_next` / `_previous` |
| `minimizeAll` / `restoreAll` / `closeAll` | `atto_ui:window_minimize_all` / `_restore_all` / `_close_all` |

- **`@atto-ui/react`**: use `<WindowOpMenuItem op="cascade" />` (optionally
  `label`/`shortcut`/`enabled`). The `WINDOW_OP_MENU_IDS` map exposes the id for
  each operation.
- **Raw `setMenuBar` spec / `@atto-ui/core`**: add a `MenuItemSpec` whose `id`
  equals the reserved id; do not attach an `onClick`.

```tsx
import { Menu, WindowOpMenuItem } from '@atto-ui/react'

<Menu title="Window">
  <WindowOpMenuItem op="cascade" />
  <WindowOpMenuItem op="tile" />
  <WindowOpMenuItem op="closeAll" label="Close all" />
</Menu>
```

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
