# @atto-ui/core

Typed CommonJS facade over the atto-ui native N-API binding. It loads the
correct platform `.node` binary, exposes a strongly typed `AppHost`, and ships
spec builders for constructing runtime component trees without hand-writing
JSON.

If you want a React/JSX experience, use [`@atto-ui/react`](../react), which is
built on top of this package. Use `@atto-ui/core` directly when you want full
control over the polling loop and component specs.

## Install

```sh
npm install @atto-ui/core
```

The matching native binary (`@atto-ui/node-<platform>`) is pulled in as an
optional dependency. For local development inside this repo the package falls
back to the binary in `crates/atto-ui-node`.

## Quick start

```ts
import { AppHost, VStack, Label, Button } from '@atto-ui/core'

const host = new AppHost({ headless: false })

const callback = host.allocCallback()
const windowId = host.addDynamicWindow('Hello', [2, 1, 40, 8],
  VStack([
    Label('Hello from @atto-ui/core'),
    Button('OK', { events: { click: callback } }),
  ]),
)

// Drive the non-blocking poll loop yourself.
function tick() {
  if (!host.step()) return host.dispose()
  for (const event of host.drainCallbacks()) {
    if (event.callbackId === callback) console.log('clicked', event.targetId)
  }
  setImmediate(tick)
}
tick()
```

## What's in the box

- **`AppHost`** — owns the terminal session, desktop, windows, runtime
  component tree, and the callback queue. See [`docs/NODE_API.md`](../../docs/NODE_API.md)
  for the full method list.
- **Spec builders** (`src/builders.ts`) — typed helpers that return
  `ComponentSpec` objects: `VStack` / `HStack` / `Grid`, `Text` / `Label` /
  `Button` / `TextBox` / `Checkbox` / `RadioGroup` / `Slider` / `ProgressBar` /
  `ListBox` / `TableView`, rich text (`RichText` / `TextSpan` / `StyledLabel`),
  `MarkdownViewer`, `TabView`, `Splitter`, chat builders, and more.
- **Types** — `ComponentSpec`, `ComponentValue`, `TreeOp`, `Rect`/`RectLike`,
  `CallbackInvocation`, and the `AppHostConfig` shape.

## Native binary resolution order

`@atto-ui/core` searches for the native `.node` in this order:

1. `ATTO_UI_NATIVE_LIBRARY_PATH`
2. `NAPI_RS_NATIVE_LIBRARY_PATH`
3. A local `atto_ui_node.<platform>.node` next to the package
4. `@atto-ui/core-<platform>` / `@atto-ui/node-<platform>` optional packages
5. `@atto-ui/node`
6. The workspace fallback at `crates/atto-ui-node` (local development)

## Docs

- API reference: [`docs/NODE_API.md`](../../docs/NODE_API.md)
- React layer: [`@atto-ui/react`](../react)
- Getting started tutorial: [`docs/REACT_GETTING_STARTED.md`](../../docs/REACT_GETTING_STARTED.md)
