# Atto UI React Agent Example

This example demonstrates the Node binding and React renderer with one app that covers:

- React state updates through a counter button.
- Controlled `TextBox` input for a todo form.
- `ListBox` selection and todo add/remove actions.
- Streaming chat updates driven by a `for await` token source while the UI tick loop stays responsive.

## Run

From the repository root, install the JS dependencies and build the native binding and React package first:

```sh
npm install --prefix packages/react
npm install --prefix examples/node --omit=optional
cd crates/atto-ui-node
npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform
cd ../..
npm run build --prefix packages/react
```

Then run the interactive example:

```sh
node examples/node/agent_chat.cjs
```

Press `Ctrl+Q` to exit the TUI.

## Headless Smoke

The same example has a deterministic headless mode for CI/manual smoke checks:

```sh
ATTO_UI_EXAMPLE_HEADLESS=1 node examples/node/agent_chat.cjs --fast
```

Or use the package script:

```sh
npm run headless --prefix examples/node
```

It prints a compact snapshot summary after the mock stream finishes.
Headless mode also sends native key events to click the counter button and add a todo through the controlled input, so the smoke covers state, events, controlled input, and streaming.

## Stress Smoke

The stress mode keeps the same native event path while rendering a larger todo list and a high-frequency mock stream:

```sh
npm run stress --prefix examples/node
```

Useful knobs:

- `--stress` or `ATTO_UI_EXAMPLE_STRESS=1`: enable stress defaults.
- `--todo-count=500` or `ATTO_UI_EXAMPLE_TODO_COUNT=500`: initial todo list size.
- `--stress-tokens=1500` or `ATTO_UI_EXAMPLE_STRESS_TOKENS=1500`: prompt token count for the mock stream.

The example batches fast mock token delivery and visible chat refreshes so long streams keep the UI tick loop responsive.

## LLM SDK Providers

The default provider is `mock`, so the example runs offline. To feed real SDK tokens through the same UI path, install the desired SDK in this example directory and set the provider:

```sh
npm install --prefix examples/node openai
OPENAI_API_KEY=... ATTO_UI_CHAT_PROVIDER=openai node examples/node/agent_chat.cjs
```

```sh
npm install --prefix examples/node @anthropic-ai/sdk
ANTHROPIC_API_KEY=... ATTO_UI_CHAT_PROVIDER=anthropic node examples/node/agent_chat.cjs
```

Optional environment variables:

- `ATTO_UI_CHAT_PROMPT`: initial prompt text.
- `OPENAI_MODEL`: OpenAI model, default `gpt-4o-mini`.
- `ANTHROPIC_MODEL`: Anthropic model, default `claude-3-5-haiku-latest`.
- `ATTO_UI_EXAMPLE_AUTOSTART=0`: disable automatic initial stream.
- `ATTO_UI_EXAMPLE_TOKEN_DELAY_MS`: mock token delay in milliseconds.
- `ATTO_UI_EXAMPLE_TODO_COUNT`: initial todo count for smoke/stress runs.
- `ATTO_UI_EXAMPLE_STRESS_TOKENS`: mock stress prompt token count.
