/**
 * 11 — Chat (agent transcript)
 *
 * Shows the block-based chat binding: `ChatPanel` (a `ChatMessageList` above a
 * `ChatInputPanel`) driven by the `useChatMessages` hook. The seeded transcript
 * renders every block kind (thinking / tool_use+tool_result / diff / plan /
 * todo / notice). On mount a mock assistant turn streams in token-by-token; the
 * input submits user turns, and the inline approve / diff / plan controls feed
 * their decisions back through the hook.
 * Run interactively:  npm run chat
 * Headless smoke:      ATTO_UI_EXAMPLE_HEADLESS=1 npm run chat
 */
import { useEffect, useState } from 'react'
import {
  ChatDiffBlock,
  ChatMessage,
  ChatNoticeBlock,
  ChatPanel,
  ChatPlanBlock,
  ChatTextBlock,
  ChatThinkingBlock,
  ChatTodoBlock,
  ChatToolAnsiOutput,
  ChatToolResultBlock,
  ChatToolTextInput,
  ChatToolUseBlock,
  Text,
  VStack,
  Window,
  useChatMessages,
  type AttoUiCallbackEvent,
  type ChatMessagesStore,
} from '@atto-ui/react'

import { startDemo, waitFor, hasText } from './_runtime'

function seedMessages() {
  return [
    ChatMessage(1, [ChatTextBlock(101, '欢迎使用 Atto UI 的 React Chat 示例。')], { role: 'system' }),
    ChatMessage(2, [ChatTextBlock(201, '帮我看看仓库结构。')], { role: 'user' }),
    ChatMessage(
      3,
      [
        ChatThinkingBlock(301, '先用 ls 看目录，再总结。', { collapsed: true }),
        ChatTextBlock(302, '我先看一下目录：'),
        ChatToolUseBlock(303, 'call-ls', 'bash', {
          input: ChatToolTextInput('ls -la'),
          status: 'done',
        }),
        ChatToolResultBlock(304, 'call-ls', {
          output: ChatToolAnsiOutput('src/\npackages/\nCargo.toml'),
          exitCode: 0,
        }),
        ChatTextBlock(305, '主要有 `src/`、`packages/` 等目录。'),
      ],
      { role: 'assistant', meta: { model: 'claude-demo', usage: { input: 642, output: 88 } } },
    ),
    // 下面几条是可交互的：点击按钮会把决策回写到 transcript。
    ChatMessage(
      4,
      [
        ChatToolUseBlock(401, 'call-rm', 'bash', {
          input: ChatToolTextInput('rm -rf target/'),
          status: 'pending',
          approval: {
            id: 'ap-1',
            prompt: '允许执行 rm -rf target/ 吗？',
            options: [
              { id: 'allow_once', label: '仅此一次' },
              { id: 'allow_always', label: '总是允许' },
              { id: 'deny', label: '拒绝' },
            ],
          },
        }),
      ],
      { role: 'assistant' },
    ),
    ChatMessage(5, [ChatDiffBlock(501, 'src/main.rs', '@@ -1 +1 @@\n-old\n+new', { decision: 'pending' })], {
      role: 'assistant',
    }),
    ChatMessage(6, [ChatPlanBlock(601, [{ text: '梳理需求' }, { text: '实现' }, { text: '测试' }], { decision: 'pending' })], {
      role: 'assistant',
    }),
    ChatMessage(
      7,
      [
        ChatTodoBlock(701, [
          { text: '设计 block 模型', state: 'done' },
          { text: '渲染各类 block', state: 'in_progress' },
          { text: '编写测试', state: 'pending' },
        ]),
      ],
      { role: 'assistant' },
    ),
    ChatMessage(8, [ChatNoticeBlock(801, 'warning', '上下文接近上限，已压缩较早的消息。')], { role: 'system' }),
  ]
}

function readMap(payload: AttoUiCallbackEvent['payload']): Record<string, unknown> {
  if (payload && typeof payload === 'object') {
    if ('$type' in payload && (payload as { $type?: string }).$type === 'map') {
      return (payload as { data: Record<string, unknown> }).data
    }
    return payload as Record<string, unknown>
  }
  return {}
}

function extractText(payload: AttoUiCallbackEvent['payload']): string {
  const map = readMap(payload)
  return typeof map.text === 'string' ? map.text : ''
}

/** Stream an assistant text turn token-by-token; resolves when complete. */
function streamReply(chat: ChatMessagesStore, text: string, stepMs: number): Promise<void> {
  const { messageId, blockId } = chat.addTextTurn('assistant', '', {
    status: 'streaming',
    meta: { model: 'claude-demo' },
  })
  const chars = Array.from(text)
  return new Promise((resolve) => {
    let i = 0
    const timer = setInterval(() => {
      if (i < chars.length) {
        chat.appendTextDelta(blockId, chars[i])
        i += 1
        return
      }
      chat.setTurnStatus(messageId, 'complete')
      chat.setMeta(messageId, {
        model: 'claude-demo',
        usage: { input: 320, output: chars.length },
        elapsed_ms: 600,
        stop_reason: 'end_turn',
      })
      clearInterval(timer)
      resolve()
    }, stepMs)
  })
}

function App({ stepMs = 35, onReady }: { stepMs?: number; onReady?: () => void }) {
  const chat = useChatMessages(seedMessages())
  const [note, setNote] = useState('chat: 启动中…')

  // Auto-stream one assistant turn on mount so streaming is visible immediately.
  useEffect(() => {
    let cancelled = false
    void (async () => {
      await streamReply(chat, '这是一段流式回复：逐字输出，结束后回填用量元数据。', stepMs)
      if (cancelled) return
      setNote('chat: 已就绪（输入消息回车发送）')
      onReady?.()
    })()
    return () => {
      cancelled = true
    }
    // chat mutators are stable (useCallback); run once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  function onSubmit(event: AttoUiCallbackEvent) {
    const text = extractText(event.payload).trim()
    if (text.length === 0) return
    chat.push({
      id: chat.nextMessageId(),
      role: 'user',
      status: 'complete',
      blocks: [{ type: 'text', block_id: chat.nextBlockId(), markdown: text }],
    })
    setNote('chat: 生成中…')
    void streamReply(chat, `收到：「${text}」。这是流式回复。`, stepMs).then(() =>
      setNote('chat: 已就绪'),
    )
  }

  function onApprove(event: AttoUiCallbackEvent) {
    const map = readMap(event.payload)
    chat.resolveApproval(Number(map.block_id), String(map.option_id))
    setNote(`审批：${String(map.option_id)}`)
  }

  function onEditDecision(event: AttoUiCallbackEvent) {
    const map = readMap(event.payload)
    chat.setEditDecision(Number(map.block_id), map.decision as 'accepted' | 'rejected' | 'pending')
    setNote(`Diff：${String(map.decision)}`)
  }

  function onPlanDecision(event: AttoUiCallbackEvent) {
    const map = readMap(event.payload)
    chat.setPlanDecision(Number(map.block_id), map.decision as 'accepted' | 'rejected' | 'pending')
    setNote(`Plan：${String(map.decision)}`)
  }

  function onCancel(event: AttoUiCallbackEvent) {
    const map = readMap(event.payload)
    chat.setTurnStatus(Number(map.message_id), 'canceled')
    setNote('已中断生成')
  }

  function onMessageAction(event: AttoUiCallbackEvent) {
    const map = readMap(event.payload)
    setNote(`消息操作：${String(map.kind)}`)
  }

  return (
    <Window title="React Chat" rect={[1, 1, 78, 22]}>
      <VStack spacing={1}>
        <Text>{note}</Text>
        <ChatPanel
          layout={{ height: 'fill' }}
          spacing={1}
          list={{
            messages: chat.messages,
            fillWidth: true,
            onApprove,
            onEditDecision,
            onPlanDecision,
            onCancel,
            onMessageAction,
          }}
          input={{ mode: { kind: 'text', title: '消息' }, clearOnSubmit: true, onSubmit }}
        />
      </VStack>
    </Window>
  )
}

if (process.env.ATTO_UI_EXAMPLE_HEADLESS === '1') {
  let resolveReady: () => void
  const ready = new Promise<void>((resolve) => {
    resolveReady = resolve
  })
  startDemo(<App stepMs={2} onReady={() => resolveReady()} />, {
    singleWindow: false,
    idPrefix: 'chat',
    async headlessProbe(handle) {
      await ready
      await waitFor(() => hasText(handle, 'chat: 已就绪'), 'initial stream completion')
    },
  })
} else {
  startDemo(<App />, { singleWindow: false, idPrefix: 'chat' })
}
