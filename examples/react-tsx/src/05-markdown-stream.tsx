/**
 * 05 — Streaming markdown
 *
 * Renders a `Markdown` viewer whose content grows from an async token stream,
 * the way an LLM SDK (`for await (const chunk of stream)`) would feed it. The
 * non-blocking tick loop keeps the UI responsive while tokens arrive.
 * Run interactively:  npm run markdown
 * Headless smoke:      ATTO_UI_EXAMPLE_HEADLESS=1 npm run markdown
 */
import { useEffect, useState } from 'react'
import { Markdown, Text, VStack, Window } from '@atto-ui/react'

import { startDemo, delay, waitFor, hasText } from './_runtime'

const TOKENS = [
  '# Streaming demo\n\n',
  'Tokens arrive ',
  'one chunk ',
  'at a time.\n\n',
  '- **bold** and `code`\n',
  '- lists render live\n\n',
  '> Done.',
]

async function* mockStream(delayMs: number): AsyncGenerator<string> {
  for (const token of TOKENS) {
    await delay(delayMs)
    yield token
  }
}

function App({ delayMs = 60, onDone }: { delayMs?: number; onDone?: () => void }) {
  const [content, setContent] = useState('')
  const [done, setDone] = useState(false)

  useEffect(() => {
    let cancelled = false
    void (async () => {
      for await (const token of mockStream(delayMs)) {
        if (cancelled) return
        setContent((current) => current + token)
      }
      if (!cancelled) {
        setDone(true)
        onDone?.()
      }
    })()
    return () => {
      cancelled = true
    }
  }, [delayMs, onDone])

  return (
    <Window title="Assistant" rect={[2, 1, 50, 16]}>
      <VStack spacing={1} padding={1}>
        <Text>{done ? 'stream: complete' : 'stream: receiving...'}</Text>
        <Markdown markdown={content} />
      </VStack>
    </Window>
  )
}

if (process.env.ATTO_UI_EXAMPLE_HEADLESS === '1') {
  let resolveDone: () => void
  const streamDone = new Promise<void>((resolve) => {
    resolveDone = resolve
  })
  startDemo(<App delayMs={5} onDone={() => resolveDone()} />, {
    singleWindow: false,
    idPrefix: 'markdown',
    async headlessProbe(handle) {
      await streamDone
      await waitFor(() => hasText(handle, 'stream: complete'), 'stream completion')
    },
  })
} else {
  startDemo(<App />, { singleWindow: false, idPrefix: 'markdown' })
}
