# Atto UI React Agent Example

This example demonstrates the Node binding and React renderer with one app that covers:

- React state updates through a counter button.
- Controlled `TextBox` input for a todo form.
- `ListBox` selection and todo add/remove actions.
- Streaming chat updates driven by a `for await` token source while the UI tick loop stays responsive.

## Run

From the repository root, build the native binding and React package first:

```sh
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

It prints a compact snapshot summary after the mock stream finishes.
Headless mode also sends native key events to click the counter button and add a todo through the controlled input, so the smoke covers state, events, controlled input, and streaming.

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
