'use strict'

function component(type, options = {}) {
  return makeSpec(type, options.id, options.props, options.events, options.children)
}

function child(node, options = {}) {
  if (options.layout === undefined && isEmptyRecord(options.meta)) return node
  const out = { node }
  if (options.layout !== undefined) out.layout = options.layout
  if (!isEmptyRecord(options.meta)) out.meta = options.meta
  return out
}

function withLayout(node, layout, meta) {
  return child(node, { layout, meta })
}

function withMeta(node, meta) {
  return child(node, { meta })
}

function tab(title, node, layout) {
  return child(node, { layout, meta: { title } })
}

function Text(text, options = {}) {
  return makeSpec('Text', options.id, {
    text,
    selectable: options.selectable,
    clipboard: options.clipboard,
  }, options.events)
}

function Label(text, options = {}) {
  return makeSpec('Label', options.id, {
    text,
    enabled: enabledValue(options),
  }, options.events)
}

function Button(first = {}, second = {}) {
  const options = typeof first === 'string' ? { ...second, label: first } : first
  return makeSpec('Button', options.id, {
    label: options.label ?? options.text,
    enabled: enabledValue(options),
  }, events(options.events, { click: options.onClick }))
}

function TextBox(options = {}) {
  return makeSpec('TextBox', options.id, {
    title: options.title,
    text: options.text,
    border: options.border,
    placeholder: options.placeholder,
    clipboard: options.clipboard,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange, submit: options.onSubmit }))
}

function TextArea(options = {}) {
  return makeSpec('TextArea', options.id, {
    title: options.title,
    text: options.text,
    height: options.height,
    enter_submits: options.enterSubmits,
    placeholder: options.placeholder,
    clipboard: options.clipboard,
    kill_ring: options.killRing,
    history: options.history,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange, submit: options.onSubmit }))
}

function Checkbox(options = {}) {
  return makeSpec('Checkbox', options.id, {
    label: options.label,
    checked: options.checked,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange }))
}

function RadioGroup(options = {}) {
  return makeSpec('RadioGroup', options.id, {
    label: options.label,
    options: options.options,
    selection: options.selection,
    height: options.height,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange }))
}

function Slider(options = {}) {
  return makeSpec('Slider', options.id, {
    min: options.min,
    max: options.max,
    value: options.value,
    step: options.step,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange }))
}

function Spinner(textOrOptions = {}, options = {}) {
  const props = typeof textOrOptions === 'string' ? { ...options, text: textOrOptions } : textOrOptions
  return makeSpec('Spinner', props.id, {
    text: props.text,
    running: props.running,
    enabled: enabledValue(props),
  }, props.events)
}

function ProgressBar(options = {}) {
  return makeSpec('ProgressBar', options.id, {
    min: options.min,
    max: options.max,
    value: options.value,
    show_text: options.showText,
    text: options.text,
    enabled: enabledValue(options),
  }, options.events)
}

function ListBox(options = {}) {
  return makeSpec('ListBox', options.id, {
    title: options.title,
    items: options.items,
    selection: options.selection,
    height: options.height,
    border: options.border,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange }))
}

function TableView(options = {}) {
  return makeSpec('TableView', options.id, {
    title: options.title,
    headers: options.headers,
    rows: options.rows,
    selection: options.selection,
    height: options.height,
    border: options.border,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange }))
}

function VStack(first = {}, second) {
  const args = containerArgs(first, second)
  return makeSpec('VStack', args.options.id, stackProps(args.options), args.options.events, args.children)
}

function HStack(first = {}, second) {
  const args = containerArgs(first, second)
  return makeSpec('HStack', args.options.id, stackProps(args.options), args.options.events, args.children)
}

function Grid(first = {}, second) {
  const args = containerArgs(first, second)
  const options = args.options
  return makeSpec('Grid', options.id, {
    columns: options.columns,
    row_gap: options.rowGap,
    column_gap: options.columnGap,
    padding: options.padding,
    scrollable: options.scrollable,
  }, options.events, args.children)
}

function Border(node, options = {}) {
  return makeSpec('Border', options.id, { border: options.border }, options.events, [node])
}

function Visibility(node, options = {}) {
  return makeSpec('Visibility', options.id, { visible: options.visible }, options.events, [node])
}

function Divider(orientationOrOptions = {}, options = {}) {
  const props = typeof orientationOrOptions === 'string' ? { ...options, orientation: orientationOrOptions } : orientationOrOptions
  return makeSpec('Divider', props.id, { orientation: props.orientation }, props.events)
}

function Spacer(options = {}) {
  return makeSpec('Spacer', options.id, undefined, options.events)
}

function Splitter(first, second, options = {}) {
  return makeSpec('Splitter', options.id, {
    orientation: options.orientation,
    split_pos: options.splitPos,
    min_first: options.minFirst,
    min_second: options.minSecond,
    border: options.border,
  }, options.events, [first, second])
}

function TabView(first = {}, second) {
  const args = containerArgs(first, second)
  const options = args.options
  return makeSpec('TabView', options.id, {
    selection: options.selection,
    header_position: options.headerPosition,
  }, events(options.events, { change: options.onChange }), args.children)
}

function TextSpan(text, options = {}) {
  return makeSpec('TextSpan', options.id, {
    text,
    bold: options.bold,
    italic: options.italic,
    underline: options.underline,
    strike: options.strike,
    color: options.color,
    href: options.href,
  }, options.events)
}

function RichText(first = {}, second) {
  const args = containerArgs(first, second)
  const options = args.options
  return makeSpec('RichText', options.id, undefined, events(options.events, { link: options.onLink }), args.children)
}

function StyledLabel(text, options = {}) {
  return makeSpec('StyledLabel', options.id, {
    text,
    enabled: enabledValue(options),
  }, events(options.events, { link: options.onLink }))
}

function Disclosure(options = {}, children) {
  return makeSpec('Disclosure', options.id, {
    title: options.title,
    content: options.content,
    expanded: options.expanded,
    status: options.status,
    enabled: enabledValue(options),
  }, events(options.events, { toggle: options.onToggle }), children ?? options.children)
}

function TypeAhead(options = {}) {
  return makeSpec('TypeAhead', options.id, typeAheadProps(options), typeAheadEvents(options))
}

function CommandPalette(options = {}) {
  return makeSpec('CommandPalette', options.id, typeAheadProps(options), typeAheadEvents(options))
}

function MarkdownViewer(first = {}, second = {}) {
  const options = typeof first === 'string' ? { ...second, markdown: first } : first
  return makeSpec('MarkdownViewer', options.id, {
    markdown: options.markdown ?? options.text,
    wrap_width: options.wrapWidth,
    show_markers: options.showMarkers,
    vertical_scrollbar: options.verticalScrollbar,
    code_block_max_height: options.codeBlockMaxHeight,
    table_max_height: options.tableMaxHeight,
  }, events(options.events, { link: options.onLink }))
}

function TerminalEmulator(options = {}) {
  return makeSpec('TerminalEmulator', options.id, {
    command: options.command,
    args: options.args,
    scrollback_len: options.scrollbackLen,
    capture: options.capture,
    capture_on_click: options.captureOnClick,
    scroll_step: options.scrollStep,
  }, events(options.events, { input: options.onInput, close: options.onClose }))
}

function FileTreeNode(id, name, options = {}) {
  return fileTreeNodeValue({ ...options, id, name })
}

function FileTree(options = {}) {
  const nodes = options.nodes ?? options.roots
  return makeSpec('FileTree', options.id, {
    title: options.title,
    border: options.border,
    nodes: nodes?.map(fileTreeNodeValue),
    selection: options.selection,
    height: options.height,
    icons: fileTreeIconsValue(options.icons),
    enabled: enabledValue(options),
  }, events(options.events, {
    select: options.onSelect,
    rename: options.onRename,
    delete: options.onDelete,
  }))
}

function fileTreeIconsValue(icons) {
  if (icons === undefined) return undefined
  const out = {}
  for (const [ext, icon] of Object.entries(icons)) {
    out[ext] = typeof icon === 'string'
      ? icon
      : { glyph: icon.glyph, ...(icon.color !== undefined ? { color: icon.color } : {}) }
  }
  return out
}

function ChatMessage(messageId, blocks, options = {}) {
  const meta = chatMetaValue(options)
  return compactRecord({
    id: messageId,
    role: options.role ?? 'assistant',
    status: options.status ?? 'complete',
    meta,
    blocks,
  }) ?? {}
}

function ChatTextBlock(blockId, markdown, options = {}) {
  return compactRecord({
    type: 'text',
    block_id: blockId,
    markdown,
    streaming: options.streaming,
  }) ?? {}
}

function ChatThinkingBlock(blockId, markdown, options = {}) {
  return compactRecord({
    type: 'thinking',
    block_id: blockId,
    markdown,
    streaming: options.streaming,
    collapsed: options.collapsed,
  }) ?? {}
}

function ChatToolTextInput(text) {
  return { text }
}

function ChatToolJsonInput(json) {
  return { json }
}

function ChatToolAnsiOutput(ansi) {
  return { ansi }
}

function ChatToolMarkdownOutput(markdown) {
  return { markdown }
}

function ChatToolDiffOutput(diff) {
  return { diff }
}

function ChatApprovalOption(id, label, options = {}) {
  return compactRecord({
    id,
    label,
    action: options.action,
    level: options.level,
  }) ?? {}
}

function ChatApprovalRequest(id, prompt, options, requestOptions = {}) {
  return compactRecord({
    id,
    prompt,
    options,
    resolved: requestOptions.resolved,
    resolved_action: requestOptions.resolvedAction,
    resolved_level: requestOptions.resolvedLevel,
  }) ?? {}
}

function ChatToolUseBlock(blockId, callId, name, options = {}) {
  return compactRecord({
    type: 'tool_use',
    block_id: blockId,
    call_id: callId,
    name,
    input: options.input ?? ChatToolTextInput(''),
    status: options.status ?? 'pending',
    approval: options.approval,
    collapsed: options.collapsed,
  }) ?? {}
}

function ChatToolResultBlock(blockId, callId, options = {}) {
  return compactRecord({
    type: 'tool_result',
    block_id: blockId,
    call_id: callId,
    ok: options.ok ?? true,
    exit_code: options.exitCode,
    output: options.output ?? ChatToolAnsiOutput(''),
    collapsed: options.collapsed,
  }) ?? {}
}

function ChatDiffBlock(blockId, path, diff, options = {}) {
  return compactRecord({
    type: 'diff',
    block_id: blockId,
    path,
    diff,
    decision: options.decision ?? 'pending',
  }) ?? {}
}

function ChatPlanBlock(blockId, items, options = {}) {
  return compactRecord({
    type: 'plan',
    block_id: blockId,
    items,
    decision: options.decision ?? 'pending',
  }) ?? {}
}

function ChatTaskTranscriptItem(role, blocks) {
  return { role, blocks }
}

function ChatTaskBlock(blockId, title, options = {}) {
  return compactRecord({
    type: 'task',
    block_id: blockId,
    title,
    status: options.status ?? 'pending',
    summary: options.summary ?? '',
    transcript: options.transcript ?? [],
    collapsed: options.collapsed,
  }) ?? {}
}

function ChatTodoBlock(blockId, items) {
  return compactRecord({ type: 'todo', block_id: blockId, items }) ?? {}
}

function ChatAttachmentBlock(blockId, name, options = {}) {
  return compactRecord({
    type: 'attachment',
    block_id: blockId,
    name,
    url: options.url,
    mime: options.mime,
  }) ?? {}
}

function ChatNoticeBlock(blockId, level, text) {
  return compactRecord({ type: 'notice', block_id: blockId, level, text }) ?? {}
}

function ChatCompactBlock(blockId, status, options = {}) {
  return compactRecord({
    type: 'compact',
    block_id: blockId,
    status,
    before_tokens: options.beforeTokens,
    after_tokens: options.afterTokens,
    summary: options.summary ?? '',
  }) ?? {}
}

function ChatArtifactBlock(blockId, options) {
  return compactRecord({
    type: 'artifact',
    block_id: blockId,
    kind: options.kind,
    anchor: options.anchor,
    title: options.title,
  }) ?? {}
}

function ChatTextMessage(messageId, markdown, options = {}) {
  return ChatMessage(messageId, [
    ChatTextBlock(options.blockId ?? derivedChatBlockId(messageId, 0), markdown, options),
  ], options)
}

function ChatFileMessage(messageId, name, options = {}) {
  return ChatMessage(messageId, [
    ChatAttachmentBlock(options.blockId ?? derivedChatBlockId(messageId, 0), name, options),
  ], options)
}

function ChatToolCallMessage(messageId, name, options = {}) {
  const callId = options.callId ?? `tool-${messageId}`
  const toolStatus = options.toolStatus ?? 'running'
  const turnStatus = toolStatus === 'pending' || toolStatus === 'running' ? 'streaming' : 'complete'
  const blocks = [
    ChatToolUseBlock(options.toolUseBlockId ?? derivedChatBlockId(messageId, 0), callId, name, {
      input: options.input,
      status: toolStatus,
      approval: options.approval,
      collapsed: options.toolUseCollapsed,
    }),
  ]
  if (options.output !== undefined && options.output !== '') {
    blocks.push(ChatToolResultBlock(options.toolResultBlockId ?? derivedChatBlockId(messageId, 1), callId, {
      ok: options.ok ?? toolStatus !== 'error',
      exitCode: options.exitCode,
      output: chatToolOutputFromString(options.outputKind ?? 'ansi', options.output),
      collapsed: options.toolResultCollapsed,
    }))
  }
  return ChatMessage(messageId, blocks, { ...options, status: options.status ?? turnStatus })
}

function ChatArtifactMessage(messageId, options) {
  return ChatMessage(messageId, [
    ChatArtifactBlock(options.blockId ?? derivedChatBlockId(messageId, 0), options),
  ], options)
}

function ChatMessageList(options = {}) {
  return makeSpec('ChatMessageList', options.id, {
    messages: options.messages,
    spacing: options.spacing,
    padding: options.padding,
    wrap_width: options.wrapWidth,
    show_timestamps: options.showTimestamps,
    bubble_width_percent: options.bubbleWidthPercent,
    auto_scroll: options.autoScroll,
  }, events(options.events, {
    load_more: options.onLoadMore,
    open_artifact: options.onOpenArtifact,
    approve: options.onApprove,
    edit_decision: options.onEditDecision,
    plan_decision: options.onPlanDecision,
    cancel: options.onCancel,
    message_action: options.onMessageAction,
  }))
}

function ChatInputMode(mode = 'text', options = {}) {
  const title = options.title ?? 'Input'
  const prompt = options.prompt ?? (['choice', 'confirm'].includes(normalizeName(mode)) ? title : undefined)
  return compactRecord({
    type: mode,
    title,
    prompt,
    placeholder: options.placeholder,
    height: options.height,
    options: options.options,
    allow_custom: options.allowCustom,
    submit_label: options.submitLabel,
    yes_label: options.yesLabel,
    no_label: options.noLabel,
  }) ?? {}
}

function ChatSlashCommand(label, options = {}) {
  return compactRecord({
    id: options.id,
    label,
    detail: options.detail,
    replacement: options.replacement,
    action: options.action,
  }) ?? {}
}

function ChatMentionCandidate(label, options = {}) {
  return compactRecord({
    id: options.id,
    label,
    detail: options.detail,
    replacement: options.replacement,
  }) ?? {}
}

function ChatInputPanel(options = {}) {
  return makeSpec('ChatInputPanel', options.id, {
    mode: options.mode ?? ChatInputMode(),
    draft: options.draft,
    custom: options.custom,
    history: options.history,
    slash_commands: options.slashCommands,
    mention_candidates: options.mentionCandidates,
    selection: options.selection,
    enabled: enabledValue(options),
    clear_on_submit: options.clearOnSubmit,
  }, events(options.events, {
    submit: options.onSubmit,
    slash_command: options.onSlashCommand,
    mention_query: options.onMentionQuery,
  }))
}

function makeSpec(type, id, props, eventInput, children) {
  const spec = { type }
  const compactProps = compactRecord(props)
  const compactEvents = compactEventRecord(eventInput)
  if (id !== undefined) spec.id = id
  if (compactProps !== undefined) spec.props = compactProps
  if (compactEvents !== undefined) spec.events = compactEvents
  if (children !== undefined && children.length > 0) spec.children = children
  return spec
}

function compactRecord(record) {
  if (record === undefined) return undefined
  const out = {}
  for (const [key, value] of Object.entries(record)) {
    if (value !== undefined) out[key] = value
  }
  return Object.keys(out).length > 0 ? out : undefined
}

function compactEventRecord(record) {
  if (record === undefined) return undefined
  const out = {}
  for (const [key, value] of Object.entries(record)) {
    if (value !== undefined) out[key] = value
  }
  return Object.keys(out).length > 0 ? out : undefined
}

function events(base, aliases) {
  const merged = { ...(base ?? {}) }
  for (const [key, value] of Object.entries(aliases)) {
    if (value !== undefined) merged[key] = value
  }
  return compactEventRecord(merged)
}

function enabledValue(options) {
  if (options.enabled !== undefined) return options.enabled
  if (options.disabled !== undefined) return !options.disabled
  return undefined
}

function containerArgs(first, second) {
  if (Array.isArray(first)) {
    return { options: {}, children: first }
  }
  const options = first ?? {}
  return { options, children: second ?? options.children }
}

function stackProps(options) {
  return {
    spacing: options.spacing,
    padding: options.padding,
    scrollable: options.scrollable,
  }
}

function typeAheadProps(options) {
  return {
    title: options.title,
    query: options.query,
    items: options.items,
    selection: options.selection,
    accepted: options.accepted,
    open: options.open,
    open_on_empty: options.openOnEmpty,
    placeholder: options.placeholder,
    height: options.height,
    max_results: options.maxResults,
    enabled: enabledValue(options),
  }
}

function typeAheadEvents(options) {
  return events(options.events, {
    change: options.onChange,
    accept: options.onAccept,
    close: options.onClose,
  })
}

function fileTreeNodeValue(node) {
  if (typeof node.id !== 'number' || typeof node.name !== 'string') return node
  const children = node.children ?? node.nodes
  return compactRecord({
    id: node.id,
    name: node.name,
    kind: node.kind,
    children: children?.map(fileTreeNodeValue),
    expanded: node.expanded ?? node.isExpanded,
  }) ?? {}
}

function chatMetaValue(options) {
  return compactRecord({
    timestamp: options.timestamp !== undefined ? options.timestamp : options.meta?.timestamp,
    model: options.meta?.model,
    usage: options.meta?.usage,
    elapsed_ms: options.meta?.elapsed_ms,
    stop_reason: options.meta?.stop_reason,
  })
}

function derivedChatBlockId(messageId, ordinal) {
  return messageId * 1000 + ordinal + 1
}

function chatToolOutputFromString(kind, output) {
  switch (kind) {
    case 'ansi':
      return ChatToolAnsiOutput(output)
    case 'markdown':
      return ChatToolMarkdownOutput(output)
    case 'diff':
      return ChatToolDiffOutput(output)
  }
}

function normalizeName(name) {
  return Array.from(name).filter((char) => char !== '_' && char !== '-' && char !== ' ').join('').toLowerCase()
}

function isEmptyRecord(record) {
  return record === undefined || Object.keys(record).length === 0
}

module.exports = {
  component,
  child,
  withLayout,
  withMeta,
  tab,
  Text,
  Label,
  Button,
  TextBox,
  TextArea,
  Checkbox,
  RadioGroup,
  Slider,
  Spinner,
  ProgressBar,
  ListBox,
  TableView,
  VStack,
  HStack,
  Grid,
  Border,
  Visibility,
  Divider,
  Spacer,
  Splitter,
  TabView,
  TextSpan,
  RichText,
  StyledLabel,
  Disclosure,
  TypeAhead,
  CommandPalette,
  MarkdownViewer,
  TerminalEmulator,
  FileTreeNode,
  FileTree,
  fileTreeNodeValue,
  ChatMessage,
  ChatTextBlock,
  ChatThinkingBlock,
  ChatToolTextInput,
  ChatToolJsonInput,
  ChatToolAnsiOutput,
  ChatToolMarkdownOutput,
  ChatToolDiffOutput,
  ChatApprovalOption,
  ChatApprovalRequest,
  ChatToolUseBlock,
  ChatToolResultBlock,
  ChatDiffBlock,
  ChatPlanBlock,
  ChatTaskTranscriptItem,
  ChatTaskBlock,
  ChatTodoBlock,
  ChatAttachmentBlock,
  ChatNoticeBlock,
  ChatCompactBlock,
  ChatArtifactBlock,
  ChatTextMessage,
  ChatFileMessage,
  ChatToolCallMessage,
  ChatArtifactMessage,
  ChatMessageList,
  ChatInputMode,
  ChatSlashCommand,
  ChatMentionCandidate,
  ChatInputPanel,
}
