const React = require('react')
const { ChatInputPanel, ChatMessageList, VStack, useChatMessages, render } = require('../dist')

function extractText(payload) {
  const map = payload && payload.$type === 'map' ? payload.data : payload
  if (map && typeof map.text === 'string') return map.text
  return ''
}

function ChatApp() {
  const store = useChatMessages()

  // Stream one assistant turn on mount to exercise useChatMessages + ChatMessageList.
  React.useEffect(() => {
    const { messageId, blockId } = store.addTextTurn('assistant', '', { status: 'streaming' })
    const chunks = ['STREAM-', 'A', 'B', 'C']
    let i = 0
    const timer = setInterval(() => {
      if (i < chunks.length) {
        store.appendTextDelta(blockId, chunks[i])
        i += 1
      } else {
        store.setTurnStatus(messageId, 'complete')
        clearInterval(timer)
      }
    }, 60)
    return () => clearInterval(timer)
    // store mutators are stable (useCallback); run once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  function handleSubmit(event) {
    const text = extractText(event.payload)
    store.push({
      id: store.nextMessageId(),
      role: 'user',
      status: 'complete',
      blocks: [{ type: 'text', block_id: store.nextBlockId(), markdown: `USER:${text}` }],
    })
  }

  return React.createElement(
    VStack,
    null,
    React.createElement(ChatMessageList, {
      messages: store.messages,
      wrapWidth: 40,
      layout: { height: 'fill' },
    }),
    React.createElement(ChatInputPanel, {
      mode: { kind: 'text', title: 'Msg' },
      onSubmit: handleSubmit,
      layout: { height: 'content' },
    }),
  )
}

render(React.createElement(ChatApp), { idPrefix: 'chat-pty' })
