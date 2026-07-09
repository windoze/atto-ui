import { useCallback, useMemo, useRef, useState } from 'react'
import type {
  ChatApprovalAction,
  ChatApprovalLevel,
  ChatBlockInput,
  ChatEditDecision,
  ChatMessageInput,
  ChatMessageMeta,
  ChatPlanDecision,
  ChatPlanItem,
  ChatRole,
  ChatTaskStatus,
  ChatTaskTranscriptItem,
  ChatTodoItem,
  ChatToolOutput,
  ChatToolResultBlock,
  ChatToolStatus,
  ChatTurnStatus,
} from '@atto-ui/core'

/**
 * Imperative-feeling helpers over an immutable `ChatMessageInput[]` held in React
 * state. Mirrors the semantics of the Rust `ChatMessageStore` so a JS/TS agent can
 * stream blocks into a `<ChatMessageList>` / `<ChatPanel>` without owning a native
 * store. Every mutator produces a new array; ids are auto-assigned and monotonic.
 */
export interface ChatMessagesStore {
  readonly messages: readonly ChatMessageInput[]
  /** Replace the whole transcript. */
  setMessages(next: readonly ChatMessageInput[]): void
  /** Allocate the next monotonic message id. */
  nextMessageId(): number
  /** Allocate the next monotonic block id. */
  nextBlockId(): number

  push(message: ChatMessageInput): void
  prepend(message: ChatMessageInput): void
  prependMany(messages: readonly ChatMessageInput[]): void
  updateMessage(id: number, updater: (message: ChatMessageInput) => ChatMessageInput): void

  /**
   * Append an assistant text turn and return the new message/block ids so callers
   * can stream deltas into it.
   */
  addTextTurn(
    role: ChatRole,
    markdown?: string,
    options?: { status?: ChatTurnStatus; meta?: ChatMessageMeta },
  ): { messageId: number; blockId: number }

  appendTextDelta(blockId: number, delta: string): void
  appendToolOutput(blockId: number, delta: string): void
  setTurnStatus(id: number, status: ChatTurnStatus): void
  setMeta(id: number, meta: ChatMessageMeta): void
  setToolStatus(blockId: number, status: ChatToolStatus): void
  upsertToolResult(callId: string, result: Omit<ChatToolResultBlock, 'type' | 'block_id' | 'call_id'>): void
  resolveApproval(blockId: number, optionId: string): void
  setEditDecision(blockId: number, decision: ChatEditDecision): void
  setPlanItems(blockId: number, items: readonly ChatPlanItem[]): void
  setPlanDecision(blockId: number, decision: ChatPlanDecision): void
  setTodo(blockId: number, items: readonly ChatTodoItem[]): void
  setTaskStatus(blockId: number, status: ChatTaskStatus): void
  setTaskSummary(blockId: number, summary: string): void
  setTaskTranscript(blockId: number, transcript: readonly ChatTaskTranscriptItem[]): void
}

export function useChatMessages(initial: readonly ChatMessageInput[] = []): ChatMessagesStore {
  const [messages, setMessagesState] = useState<readonly ChatMessageInput[]>(initial)
  const messageIdRef = useRef(seedId(initial.map((m) => m.id)))
  const blockIdRef = useRef(seedId(initial.flatMap((m) => m.blocks.map(maxBlockId))))

  const setMessages = useCallback((next: readonly ChatMessageInput[]) => {
    bumpRef(messageIdRef, next.map((m) => m.id))
    bumpRef(blockIdRef, next.flatMap((m) => m.blocks.map(maxBlockId)))
    setMessagesState(next)
  }, [])

  const nextMessageId = useCallback(() => {
    const id = messageIdRef.current
    messageIdRef.current += 1
    return id
  }, [])

  const nextBlockId = useCallback(() => {
    const id = blockIdRef.current
    blockIdRef.current += 1
    return id
  }, [])

  const push = useCallback((message: ChatMessageInput) => {
    bumpRef(messageIdRef, [message.id])
    bumpRef(blockIdRef, message.blocks.map(maxBlockId))
    setMessagesState((prev) => [...prev, message])
  }, [])

  const prepend = useCallback((message: ChatMessageInput) => {
    bumpRef(messageIdRef, [message.id])
    bumpRef(blockIdRef, message.blocks.map(maxBlockId))
    setMessagesState((prev) => [message, ...prev])
  }, [])

  const prependMany = useCallback((older: readonly ChatMessageInput[]) => {
    if (older.length === 0) return
    bumpRef(messageIdRef, older.map((m) => m.id))
    bumpRef(blockIdRef, older.flatMap((m) => m.blocks.map(maxBlockId)))
    setMessagesState((prev) => [...older, ...prev])
  }, [])

  const updateMessage = useCallback(
    (id: number, updater: (message: ChatMessageInput) => ChatMessageInput) => {
      setMessagesState((prev) => prev.map((m) => (m.id === id ? updater(m) : m)))
    },
    [],
  )

  const addTextTurn = useCallback(
    (
      role: ChatRole,
      markdown = '',
      options: { status?: ChatTurnStatus; meta?: ChatMessageMeta } = {},
    ) => {
      const messageId = nextMessageId()
      const blockId = nextBlockId()
      const status = options.status ?? 'complete'
      const message: ChatMessageInput = {
        id: messageId,
        role,
        status,
        meta: options.meta,
        blocks: [{ type: 'text', block_id: blockId, markdown, streaming: status === 'streaming' }],
      }
      setMessagesState((prev) => [...prev, message])
      return { messageId, blockId }
    },
    [nextBlockId, nextMessageId],
  )

  const appendTextDelta = useCallback((blockId: number, delta: string) => {
    if (delta.length === 0) return
    setMessagesState((prev) =>
      mapBlock(prev, blockId, (block) => {
        if (block.type === 'text' || block.type === 'thinking') {
          return { ...block, markdown: block.markdown + delta }
        }
        return block
      }),
    )
  }, [])

  const appendToolOutput = useCallback((blockId: number, delta: string) => {
    if (delta.length === 0) return
    setMessagesState((prev) =>
      mapBlock(prev, blockId, (block) =>
        block.type === 'tool_result' ? { ...block, output: appendOutput(block.output, delta) } : block,
      ),
    )
  }, [])

  const setTurnStatus = useCallback((id: number, status: ChatTurnStatus) => {
    setMessagesState((prev) => prev.map((m) => (m.id === id ? withTurnStatus(m, status) : m)))
  }, [])

  const setMeta = useCallback((id: number, meta: ChatMessageMeta) => {
    setMessagesState((prev) => prev.map((m) => (m.id === id ? { ...m, meta } : m)))
  }, [])

  const setToolStatus = useCallback((blockId: number, status: ChatToolStatus) => {
    setMessagesState((prev) =>
      mapBlock(prev, blockId, (block) =>
        block.type === 'tool_use' ? { ...block, status } : block,
      ),
    )
  }, [])

  const upsertToolResult = useCallback(
    (callId: string, result: Omit<ChatToolResultBlock, 'type' | 'block_id' | 'call_id'>) => {
      const candidateId = nextBlockId()
      setMessagesState((prev) => {
        let replaced = false
        const replacedMessages = prev.map((m) => ({
          ...m,
          blocks: m.blocks.map((b) => {
            if (b.type === 'tool_result' && b.call_id === callId) {
              replaced = true
              return { ...result, type: 'tool_result', call_id: callId, block_id: b.block_id } as ChatBlockInput
            }
            return b
          }),
        }))
        if (replaced) return replacedMessages
        return prev.map((m) => {
          if (m.blocks.some((b) => b.type === 'tool_use' && b.call_id === callId)) {
            const block: ChatBlockInput = {
              ...result,
              type: 'tool_result',
              call_id: callId,
              block_id: candidateId,
            }
            return { ...m, blocks: [...m.blocks, block] }
          }
          return m
        })
      })
    },
    [nextBlockId],
  )

  const resolveApproval = useCallback((blockId: number, optionId: string) => {
    setMessagesState((prev) =>
      mapBlock(prev, blockId, (block) => {
        if (block.type !== 'tool_use' || !block.approval) return block
        const option = block.approval.options.find((o) => o.id === optionId)
        if (!option) return block
        const action = approvalOptionAction(option)
        const level = approvalOptionLevel(option)
        const nextStatus = approvalActionStatus(action)
        const status = canAdvanceTool(block.status, nextStatus) ? nextStatus : block.status
        return {
          ...block,
          status,
          approval: {
            ...block.approval,
            resolved: optionId,
            resolved_action: action,
            resolved_level: level,
          },
        }
      }),
    )
  }, [])

  const setEditDecision = useCallback((blockId: number, decision: ChatEditDecision) => {
    setMessagesState((prev) =>
      mapBlock(prev, blockId, (block) => (block.type === 'diff' ? { ...block, decision } : block)),
    )
  }, [])

  const setPlanItems = useCallback((blockId: number, items: readonly ChatPlanItem[]) => {
    setMessagesState((prev) =>
      mapBlock(prev, blockId, (block) => (block.type === 'plan' ? { ...block, items } : block)),
    )
  }, [])

  const setPlanDecision = useCallback((blockId: number, decision: ChatPlanDecision) => {
    setMessagesState((prev) =>
      mapBlock(prev, blockId, (block) => (block.type === 'plan' ? { ...block, decision } : block)),
    )
  }, [])

  const setTodo = useCallback((blockId: number, items: readonly ChatTodoItem[]) => {
    setMessagesState((prev) =>
      mapBlock(prev, blockId, (block) => (block.type === 'todo' ? { ...block, items } : block)),
    )
  }, [])

  const setTaskStatus = useCallback((blockId: number, status: ChatTaskStatus) => {
    setMessagesState((prev) =>
      mapBlock(prev, blockId, (block) => (block.type === 'task' ? { ...block, status } : block)),
    )
  }, [])

  const setTaskSummary = useCallback((blockId: number, summary: string) => {
    setMessagesState((prev) =>
      mapBlock(prev, blockId, (block) => (block.type === 'task' ? { ...block, summary } : block)),
    )
  }, [])

  const setTaskTranscript = useCallback(
    (blockId: number, transcript: readonly ChatTaskTranscriptItem[]) => {
      setMessagesState((prev) =>
        mapBlock(prev, blockId, (block) =>
          block.type === 'task' ? { ...block, transcript } : block,
        ),
      )
    },
    [],
  )

  return useMemo<ChatMessagesStore>(
    () => ({
      messages,
      setMessages,
      nextMessageId,
      nextBlockId,
      push,
      prepend,
      prependMany,
      updateMessage,
      addTextTurn,
      appendTextDelta,
      appendToolOutput,
      setTurnStatus,
      setMeta,
      setToolStatus,
      upsertToolResult,
      resolveApproval,
      setEditDecision,
      setPlanItems,
      setPlanDecision,
      setTodo,
      setTaskStatus,
      setTaskSummary,
      setTaskTranscript,
    }),
    [
      messages,
      setMessages,
      nextMessageId,
      nextBlockId,
      push,
      prepend,
      prependMany,
      updateMessage,
      addTextTurn,
      appendTextDelta,
      appendToolOutput,
      setTurnStatus,
      setMeta,
      setToolStatus,
      upsertToolResult,
      resolveApproval,
      setEditDecision,
      setPlanItems,
      setPlanDecision,
      setTodo,
      setTaskStatus,
      setTaskSummary,
      setTaskTranscript,
    ],
  )
}

// ---- internal helpers ----

function mapBlock(
  messages: readonly ChatMessageInput[],
  blockId: number,
  f: (block: ChatBlockInput) => ChatBlockInput,
): readonly ChatMessageInput[] {
  return messages.map((message) => {
    if (!message.blocks.some((block) => block.block_id === blockId)) return message
    return {
      ...message,
      blocks: message.blocks.map((block) => (block.block_id === blockId ? f(block) : block)),
    }
  })
}

function withTurnStatus(message: ChatMessageInput, status: ChatTurnStatus): ChatMessageInput {
  const streaming = status === 'streaming'
  return {
    ...message,
    status,
    blocks: message.blocks.map((block) =>
      block.type === 'text' || block.type === 'thinking' ? { ...block, streaming } : block,
    ),
  }
}

function appendOutput(output: ChatToolOutput, delta: string): ChatToolOutput {
  if ('ansi' in output) return { ansi: output.ansi + delta }
  if ('markdown' in output) return { markdown: output.markdown + delta }
  return { diff: output.diff + delta }
}

function approvalOptionAction(option: {
  readonly id: string
  readonly label: string
  readonly action?: ChatApprovalAction
}): ChatApprovalAction {
  return option.action ?? (isDenyOption(option.id) || isDenyOption(option.label) ? 'deny' : 'allow')
}

function approvalOptionLevel(option: {
  readonly id: string
  readonly label: string
  readonly level?: ChatApprovalLevel
}): ChatApprovalLevel {
  if (option.level !== undefined) return option.level
  const value = `${option.id} ${option.label}`.toLowerCase()
  if (value.includes('project') || value.includes('workspace')) return 'project'
  if (value.includes('always') || value.includes("don't ask") || value.includes('dont ask')) {
    return 'always'
  }
  return 'once'
}

function approvalActionStatus(action: ChatApprovalAction): ChatToolStatus {
  return action === 'deny' ? 'canceled' : 'running'
}

function isDenyOption(value: string): boolean {
  const v = value.trim().toLowerCase()
  return (
    v === 'no' ||
    v.includes('deny') ||
    v.includes('reject') ||
    v.includes('decline') ||
    v.includes('cancel') ||
    v.includes('stop')
  )
}

function canAdvanceTool(current: ChatToolStatus, next: ChatToolStatus): boolean {
  if (next === 'running') return current === 'pending'
  if (next === 'canceled') return current === 'pending' || current === 'running'
  return false
}

function maxBlockId(block: ChatBlockInput): number {
  if (block.type === 'task') {
    return block.transcript
      .flatMap((item) => item.blocks.map(maxBlockId))
      .reduce((acc, id) => Math.max(acc, id), block.block_id)
  }
  return block.block_id
}

function seedId(ids: readonly number[]): number {
  return ids.reduce((acc, id) => Math.max(acc, id + 1), 1)
}

function bumpRef(ref: { current: number }, ids: readonly number[]): void {
  for (const id of ids) {
    if (id + 1 > ref.current) ref.current = id + 1
  }
}
