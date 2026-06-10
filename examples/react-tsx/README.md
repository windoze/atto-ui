# Atto UI React TSX Demos

Runnable `.tsx` demos for `@atto-ui/react`. They use real JSX syntax and run
directly through the [`tsx`](https://github.com/privatenumber/tsx) loader — no
build step or `dist/` output.

| Demo | File | Shows |
|---|---|---|
| Hello | `src/01-hello.tsx` | Minimal app: one `Window`, `VStack`, `Text` with inline `<B>`. |
| Counter | `src/02-counter.tsx` | `useState` + `Button` `onClick`. |
| Todo list | `src/03-todo-list.tsx` | Controlled `TextBox`, `ListBox` selection, add/remove. |
| Desktop | `src/04-multi-window.tsx` | Multiple `Window`s, `MenuBar`/`MenuItem`, `StatusBar`. |
| Markdown stream | `src/05-markdown-stream.tsx` | `Markdown` driven by a `for await` token stream. |
| Theme switch | `src/06-theme-switch.tsx` | Runtime theme change via `handle.host.setTheme(...)`. |
| Component gallery | `src/07-component-gallery.tsx` | All core components across several windows; `<Window>` lifecycle (onClose/onMinimize) + menu to re-create/restore windows. |

## Setup

From the repository root, build the native binding and the React package once:

```sh
# 1. Build the native N-API binding (creates atto_ui_node.<platform>.node)
cd crates/atto-ui-node
npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform
cd ../..

# 2. Build the React package (emits packages/react/dist)
npm install --prefix packages/react
npm run build --prefix packages/react

# 3. Install this example's dependencies
npm install --prefix examples/react-tsx --omit=optional
```

> `react` is pinned to the exact copy used by `@atto-ui/react`'s reconciler
> (`file:../../packages/react/node_modules/react`). A single React instance is
> required — two copies trigger the classic "Invalid hook call" error.

## Run a demo

```sh
cd examples/react-tsx
npm run hello      # or: counter | todo | desktop | markdown | theme | gallery
```

Press `Ctrl+Q` to quit the TUI.

## Run with Bun or Deno

The demos run unchanged under Bun and Deno — both execute `.tsx` directly and
support the native N-API binding. The setup above (native binding + React build
+ `npm install` for `node_modules`) is still required.

**Bun** (`bun src/...` reads `tsconfig.json` and resolves the `file:` deps):

```sh
npm run gallery:bun     # or hello:bun | counter:bun | todo:bun | desktop:bun | markdown:bun | theme:bun
# equivalently: bun src/07-component-gallery.tsx
```

**Deno** (configured by `deno.json`: `nodeModulesDir: manual`, `sloppy-imports`,
and the JSX runtime; `deno task` runs with `-A`):

```sh
npm run gallery:deno    # or hello:deno | counter:deno | …
# equivalently: deno task gallery
```

Headless mode works the same way under either runtime, e.g.
`ATTO_UI_EXAMPLE_HEADLESS=1 npm run gallery:deno`.

> Deno needs `--node-modules-dir=manual` because the example uses `file:`
> dependencies, and `--sloppy-imports` for extensionless local imports like
> `./_runtime`. Both are set in `deno.json`, so `deno task <name>` just works.

## Headless smoke

Every demo has a deterministic headless mode that drives synthetic key events
against an in-memory terminal, prints a compact snapshot, then exits:

```sh
ATTO_UI_EXAMPLE_HEADLESS=1 npm run counter
```

Run all of them at once:

```sh
npm run smoke
```

## Type checking

```sh
npx tsc --noEmit
```

## Notes for writing your own JSX

- Use `<Text>` for static labels. `<Text>{`Count: ${n}`}</Text>` renders as one
  span; `<Text>Count: {n}</Text>` splits into separate spans.
- `TextBox` is controlled: pass `value` and update it from `onChange`.
- `ListBox`/`Table` report the selected index through `onSelect`/`onChange`.
- The desktop root (`singleWindow: false`) accepts `Window`, `MenuBar`, and
  `StatusBar` children directly — see the desktop demo.
