import type {
  ComponentEvents,
  ComponentProps,
  ComponentSpec,
  ComponentSpecChild,
  ComponentValue,
  ComponentValueMap,
  EdgeInsetsSpec,
  LayoutSpec,
} from '../index'

export type CallbackHandle = string
export type ComponentChildren = readonly ComponentSpecChild[]
export type EventInput = Readonly<Record<string, CallbackHandle | undefined>>
export type PropInput = Readonly<Record<string, unknown>>

export interface BuilderBaseOptions {
  readonly id?: string
  readonly events?: EventInput
}

export interface EnabledOptions {
  readonly enabled?: boolean
  readonly disabled?: boolean
}

export interface ComponentBuilderOptions extends BuilderBaseOptions {
  readonly props?: Readonly<Record<string, ComponentValue | undefined>>
  readonly children?: ComponentChildren
}

export interface ChildOptions {
  readonly layout?: LayoutSpec
  readonly meta?: ComponentProps
}

export interface TextOptions extends BuilderBaseOptions {
  readonly selectable?: boolean
  readonly clipboard?: string
}

export interface LabelOptions extends BuilderBaseOptions, EnabledOptions {}

export interface ButtonOptions extends BuilderBaseOptions, EnabledOptions {
  readonly label?: string
  readonly text?: string
  readonly onClick?: CallbackHandle
}

export interface TextBoxOptions extends BuilderBaseOptions, EnabledOptions {
  readonly title?: string
  readonly text?: string
  /** Draw the widget's own border. Defaults to `true`. */
  readonly border?: boolean
  readonly placeholder?: string
  readonly clipboard?: string
  readonly onChange?: CallbackHandle
  readonly onSubmit?: CallbackHandle
}

export interface TextAreaOptions extends TextBoxOptions {
  readonly height?: number
  readonly enterSubmits?: boolean
  readonly killRing?: string
  readonly history?: readonly string[]
}

export interface CheckboxOptions extends BuilderBaseOptions, EnabledOptions {
  readonly label?: string
  readonly checked?: boolean
  readonly onChange?: CallbackHandle
}

export interface ChoiceOptions extends BuilderBaseOptions, EnabledOptions {
  readonly label?: string
  readonly title?: string
  readonly options?: readonly string[]
  readonly items?: readonly string[]
  readonly selection?: number
  readonly height?: number
  /** Draw the widget's own border (ListBox only). Defaults to `true`. */
  readonly border?: boolean
  readonly onChange?: CallbackHandle
}

export interface SliderOptions extends BuilderBaseOptions, EnabledOptions {
  readonly min?: number
  readonly max?: number
  readonly value?: number
  readonly step?: number
  readonly onChange?: CallbackHandle
}

export interface SpinnerOptions extends BuilderBaseOptions, EnabledOptions {
  readonly text?: string
  readonly running?: boolean
}

export interface ProgressBarOptions extends BuilderBaseOptions, EnabledOptions {
  readonly min?: number
  readonly max?: number
  readonly value?: number
  readonly showText?: boolean
  readonly text?: string
}

export interface TableViewOptions extends BuilderBaseOptions, EnabledOptions {
  readonly title?: string
  readonly headers?: readonly string[]
  readonly rows?: readonly (readonly string[])[]
  readonly selection?: number
  readonly height?: number
  /** Draw the widget's own border. Defaults to `true`. */
  readonly border?: boolean
  readonly onChange?: CallbackHandle
}

export interface ContainerOptions extends BuilderBaseOptions {
  readonly children?: ComponentChildren
}

export interface StackOptions extends ContainerOptions {
  readonly spacing?: number
  readonly padding?: EdgeInsetsSpec
  readonly scrollable?: boolean
}

export interface GridOptions extends ContainerOptions {
  readonly columns?: number
  readonly rowGap?: number
  readonly columnGap?: number
  readonly padding?: EdgeInsetsSpec
  readonly scrollable?: boolean
}

export interface BorderOptions extends BuilderBaseOptions {
  readonly border?: boolean
}

export interface VisibilityOptions extends BuilderBaseOptions {
  readonly visible?: boolean
}

export interface DividerOptions extends BuilderBaseOptions {
  readonly orientation?: 'horizontal' | 'vertical' | (string & {})
}

export interface SpacerOptions extends BuilderBaseOptions {}

export interface SplitterOptions extends BuilderBaseOptions {
  readonly orientation?: 'horizontal' | 'vertical' | (string & {})
  readonly splitPos?: number
  readonly minFirst?: number
  readonly minSecond?: number
  readonly border?: boolean
}

export interface TabViewOptions extends ContainerOptions {
  readonly selection?: number
  readonly headerPosition?: 'top' | 'bottom' | 'left' | 'right' | (string & {})
  readonly onChange?: CallbackHandle
}

export interface TextSpanOptions extends BuilderBaseOptions {
  readonly bold?: boolean
  readonly italic?: boolean
  readonly underline?: boolean
  readonly strike?: boolean
  readonly color?: string
  readonly href?: string
}

export interface RichTextOptions extends ContainerOptions {
  readonly onLink?: CallbackHandle
}

export interface StyledLabelOptions extends BuilderBaseOptions, EnabledOptions {
  readonly onLink?: CallbackHandle
}

export interface DisclosureOptions extends ContainerOptions, EnabledOptions {
  readonly title?: string
  readonly content?: string
  readonly expanded?: boolean
  readonly status?: string
  readonly onToggle?: CallbackHandle
}

export interface TypeAheadOptions extends BuilderBaseOptions, EnabledOptions {
  readonly title?: string
  readonly query?: string
  readonly items?: readonly string[]
  readonly selection?: number
  readonly accepted?: string
  readonly open?: boolean
  readonly openOnEmpty?: boolean
  readonly placeholder?: string
  readonly height?: number
  readonly maxResults?: number
  readonly onChange?: CallbackHandle
  readonly onAccept?: CallbackHandle
  readonly onClose?: CallbackHandle
}

export interface CommandPaletteOptions extends TypeAheadOptions {}

export type ScrollbarVisibility = 'always' | 'auto' | 'never' | (string & {})
export type FileTreeNodeKind = 'file' | 'dir' | 'directory' | (string & {})
export type ChatRole = 'user' | 'assistant' | 'system' | `custom:${string}`
export type ChatErrorKind = 'api' | 'tool' | 'rate_limit' | 'refusal' | 'network' | 'other'
export type StopReason = 'end_turn' | 'max_tokens' | 'tool_use' | 'stop_sequence' | 'refusal'
export type ChatTurnStatus = 'complete' | 'streaming' | 'canceled' | ChatFailedStatus
export type ChatMessageStatus = ChatTurnStatus
export type ChatToolStatus = 'pending' | 'running' | 'done' | 'error' | 'canceled'
export type ChatToolCallStatus = ChatToolStatus
export type ChatArtifactKind = 'code' | 'diff' | 'file' | (string & {})
export type ChatToolOutputKind = 'ansi' | 'markdown' | 'diff'
export type ChatEditDecision = 'pending' | 'accepted' | 'rejected'
export type ChatPlanDecision = 'pending' | 'accepted' | 'rejected'
export type ChatTaskStatus = 'pending' | 'running' | 'complete' | 'failed' | 'canceled'
export type ChatTodoState = 'pending' | 'in_progress' | 'done'
export type ChatNoticeLevel = 'info' | 'warning' | 'error'
export type ChatSlashCommandAction = 'insert' | 'submit'

export interface ChatError {
  readonly kind: ChatErrorKind
  readonly message: string
  readonly detail?: string
}

export interface ChatFailedStatus {
  readonly failed: ChatError
}

export interface ChatTokenUsage {
  readonly input: number
  readonly output: number
}

export interface ChatMessageMeta {
  readonly timestamp?: string | null
  readonly model?: string
  readonly usage?: ChatTokenUsage
  readonly elapsed_ms?: number
  readonly stop_reason?: StopReason
}

export interface ChatBlockBase {
  readonly type: string
  readonly block_id: number
}

export interface ChatTextBlock extends ChatBlockBase {
  readonly type: 'text'
  readonly markdown: string
  readonly streaming?: boolean
}

export interface ChatThinkingBlock extends ChatBlockBase {
  readonly type: 'thinking'
  readonly markdown: string
  readonly streaming?: boolean
  readonly collapsed?: boolean
}

export interface ChatToolTextInput {
  readonly text: string
}

export interface ChatToolJsonInput {
  readonly json: ComponentValue
}

export type ChatToolInput = ChatToolTextInput | ChatToolJsonInput

export interface ChatApprovalOption {
  readonly id: string
  readonly label: string
}

export interface ChatApprovalRequest {
  readonly id: string
  readonly prompt: string
  readonly options: readonly ChatApprovalOption[]
  readonly resolved?: string
}

export interface ChatToolUseBlock extends ChatBlockBase {
  readonly type: 'tool_use'
  readonly call_id: string
  readonly name: string
  readonly input: ChatToolInput
  readonly status: ChatToolStatus
  readonly approval?: ChatApprovalRequest
  readonly collapsed?: boolean
}

export interface ChatToolAnsiOutput {
  readonly ansi: string
}

export interface ChatToolMarkdownOutput {
  readonly markdown: string
}

export interface ChatToolDiffOutput {
  readonly diff: string
}

export type ChatToolOutput = ChatToolAnsiOutput | ChatToolMarkdownOutput | ChatToolDiffOutput

export interface ChatToolResultBlock extends ChatBlockBase {
  readonly type: 'tool_result'
  readonly call_id: string
  readonly ok: boolean
  readonly exit_code?: number
  readonly output: ChatToolOutput
  readonly collapsed?: boolean
}

export interface ChatDiffBlock extends ChatBlockBase {
  readonly type: 'diff'
  readonly path: string
  readonly diff: string
  readonly decision: ChatEditDecision
}

export interface ChatPlanItem {
  readonly text: string
}

export interface ChatPlanBlock extends ChatBlockBase {
  readonly type: 'plan'
  readonly items: readonly ChatPlanItem[]
  readonly decision: ChatPlanDecision
}

export interface ChatTaskTranscriptItem {
  readonly role: ChatRole
  readonly blocks: readonly ChatBlockInput[]
}

export interface ChatTaskBlock extends ChatBlockBase {
  readonly type: 'task'
  readonly title: string
  readonly status: ChatTaskStatus
  readonly summary: string
  readonly transcript: readonly ChatTaskTranscriptItem[]
  readonly collapsed?: boolean
}

export interface ChatTodoItem {
  readonly text: string
  readonly state: ChatTodoState
}

export interface ChatTodoBlock extends ChatBlockBase {
  readonly type: 'todo'
  readonly items: readonly ChatTodoItem[]
}

export interface ChatAttachmentBlock extends ChatBlockBase {
  readonly type: 'attachment'
  readonly name: string
  readonly url?: string | null
  readonly mime?: string | null
}

export interface ChatNoticeBlock extends ChatBlockBase {
  readonly type: 'notice'
  readonly level: ChatNoticeLevel
  readonly text: string
}

export interface ChatArtifactBlock extends ChatBlockBase {
  readonly type: 'artifact'
  readonly kind: ChatArtifactKind
  readonly anchor: string | number
  readonly title: string
}

export type ChatBlockInput =
  | ChatTextBlock
  | ChatThinkingBlock
  | ChatToolUseBlock
  | ChatToolResultBlock
  | ChatDiffBlock
  | ChatPlanBlock
  | ChatTaskBlock
  | ChatTodoBlock
  | ChatAttachmentBlock
  | ChatNoticeBlock
  | ChatArtifactBlock

export interface ChatMessageInput {
  readonly id: number
  readonly role: ChatRole
  readonly status: ChatTurnStatus
  readonly meta?: ChatMessageMeta
  readonly blocks: readonly ChatBlockInput[]
}

export interface ChatSlashCommandInput {
  readonly id?: string
  readonly label: string
  readonly detail?: string | null
  readonly replacement?: string
  readonly action?: ChatSlashCommandAction
}

export interface ChatMentionCandidateInput {
  readonly id?: string
  readonly label: string
  readonly detail?: string | null
  readonly replacement?: string
}

export interface ChatMentionContext {
  readonly draft: string
  readonly query: string
  readonly cursor: number
  readonly replacement_start: number
  readonly replacement_end: number
}

export interface MarkdownViewerOptions extends BuilderBaseOptions {
  readonly markdown?: string
  readonly text?: string
  readonly wrapWidth?: number
  readonly showMarkers?: boolean
  readonly verticalScrollbar?: ScrollbarVisibility
  readonly codeBlockMaxHeight?: number
  readonly tableMaxHeight?: number
  readonly onLink?: CallbackHandle
}

export interface TerminalEmulatorOptions extends BuilderBaseOptions {
  readonly command?: string
  readonly args?: readonly string[]
  readonly scrollbackLen?: number
  readonly capture?: boolean
  readonly captureOnClick?: boolean
  readonly scrollStep?: number
  readonly onInput?: CallbackHandle
  readonly onClose?: CallbackHandle
}

export type FileTreeNodeLike = FileTreeNodeInput | ComponentValueMap

export interface FileTreeNodeOptions {
  readonly kind?: FileTreeNodeKind
  readonly children?: readonly FileTreeNodeLike[]
  readonly nodes?: readonly FileTreeNodeLike[]
  readonly expanded?: boolean
  readonly isExpanded?: boolean
}

export interface FileTreeNodeInput extends FileTreeNodeOptions {
  readonly id: number
  readonly name: string
}

/**
 * A file-type icon: a bare glyph string, or `{ glyph, color }` where `color`
 * is a ratatui color string (a name like `"red"`, a hex `"#ff8800"`, or an
 * indexed `"42"`). The mapping is empty by default so plain terminals get no
 * unsupported characters; use it to opt into PowerLine / Nerd Font glyphs.
 */
export type FileTreeIconLike = string | { readonly glyph: string; readonly color?: string }

export interface FileTreeOptions extends BuilderBaseOptions, EnabledOptions {
  readonly title?: string
  /** Draw the widget's own border. Defaults to `true`. */
  readonly border?: boolean
  readonly nodes?: readonly FileTreeNodeLike[]
  readonly roots?: readonly FileTreeNodeLike[]
  readonly selection?: number | null
  readonly height?: number
  /** Map of lowercased file extension → icon (string or `{glyph,color}`). */
  readonly icons?: Readonly<Record<string, FileTreeIconLike>>
  readonly onSelect?: CallbackHandle
  readonly onRename?: CallbackHandle
  readonly onDelete?: CallbackHandle
}

export interface ChatMessageBaseOptions {
  readonly role?: ChatRole
  readonly status?: ChatTurnStatus
  readonly meta?: ChatMessageMeta
  readonly timestamp?: string | null
}

export interface ChatTextBlockOptions {
  readonly streaming?: boolean
}

export interface ChatThinkingBlockOptions extends ChatTextBlockOptions {
  readonly collapsed?: boolean
}

export interface ChatToolUseBlockOptions {
  readonly input?: ChatToolInput
  readonly status?: ChatToolStatus
  readonly approval?: ChatApprovalRequest
  readonly collapsed?: boolean
}

export interface ChatToolResultBlockOptions {
  readonly ok?: boolean
  readonly exitCode?: number
  readonly output?: ChatToolOutput
  readonly collapsed?: boolean
}

export interface ChatDiffBlockOptions {
  readonly decision?: ChatEditDecision
}

export interface ChatPlanBlockOptions {
  readonly decision?: ChatPlanDecision
}

export interface ChatTaskBlockOptions {
  readonly status?: ChatTaskStatus
  readonly summary?: string
  readonly transcript?: readonly ChatTaskTranscriptItem[]
  readonly collapsed?: boolean
}

export interface ChatTextMessageOptions extends ChatMessageBaseOptions, ChatTextBlockOptions {
  readonly blockId?: number
}

export interface ChatFileMessageOptions extends ChatMessageBaseOptions {
  readonly blockId?: number
  readonly url?: string | null
  readonly mime?: string | null
}

export interface ChatToolCallMessageOptions extends ChatMessageBaseOptions {
  readonly callId?: string
  readonly input?: ChatToolInput
  readonly output?: string
  readonly outputKind?: ChatToolOutputKind
  readonly toolStatus?: ChatToolStatus
  readonly approval?: ChatApprovalRequest
  readonly ok?: boolean
  readonly exitCode?: number
  readonly toolUseBlockId?: number
  readonly toolResultBlockId?: number
  readonly toolUseCollapsed?: boolean
  readonly toolResultCollapsed?: boolean
}

export interface ChatArtifactMessageOptions extends ChatMessageBaseOptions {
  readonly blockId?: number
  readonly kind: ChatArtifactKind
  readonly anchor: string | number
  readonly title: string
}

export interface ChatMessageListOptions extends BuilderBaseOptions {
  readonly messages?: readonly ChatMessageInput[]
  readonly spacing?: number
  readonly padding?: EdgeInsetsSpec
  readonly wrapWidth?: number
  readonly showTimestamps?: boolean
  /** Percent (1..=100) of list width a bubble may occupy. 100 = full width. Default 75. */
  readonly bubbleWidthPercent?: number
  readonly autoScroll?: boolean
  readonly onLoadMore?: CallbackHandle
  readonly onOpenArtifact?: CallbackHandle
  readonly onApprove?: CallbackHandle
  readonly onEditDecision?: CallbackHandle
  readonly onPlanDecision?: CallbackHandle
  readonly onCancel?: CallbackHandle
  readonly onMessageAction?: CallbackHandle
}

export interface ChatInputModeOptions {
  readonly title?: string
  readonly prompt?: string | null
  readonly placeholder?: string | null
  readonly height?: number
  readonly options?: readonly string[]
  readonly allowCustom?: boolean
  readonly submitLabel?: string
  readonly yesLabel?: string
  readonly noLabel?: string
}

export type ChatInputModeInput = ComponentValueMap

export interface ChatSlashCommandOptions {
  readonly id?: string
  readonly detail?: string | null
  readonly replacement?: string
  readonly action?: ChatSlashCommandAction
}

export interface ChatMentionCandidateOptions {
  readonly id?: string
  readonly detail?: string | null
  readonly replacement?: string
}

export interface ChatInputPanelOptions extends BuilderBaseOptions, EnabledOptions {
  readonly mode?: ChatInputModeInput
  readonly draft?: string
  readonly custom?: string
  readonly history?: readonly string[]
  readonly slashCommands?: readonly ChatSlashCommandInput[]
  readonly mentionCandidates?: readonly ChatMentionCandidateInput[]
  readonly selection?: number
  readonly clearOnSubmit?: boolean
  readonly onSubmit?: CallbackHandle
  readonly onSlashCommand?: CallbackHandle
  readonly onMentionQuery?: CallbackHandle
}

/** Build a raw runtime component spec for custom or less common component types. */
export function component(type: string, options: ComponentBuilderOptions = {}): ComponentSpec {
  return makeSpec(type, options.id, options.props, options.events, options.children)
}

/** Attach layout and/or meta to a node when it is used as a child. */
export function child(node: ComponentSpec, options: ChildOptions = {}): ComponentSpecChild {
  if (options.layout === undefined && isEmptyRecord(options.meta)) return node
  const out: { node: ComponentSpec; layout?: LayoutSpec; meta?: ComponentProps } = { node }
  if (options.layout !== undefined) out.layout = options.layout
  if (!isEmptyRecord(options.meta)) out.meta = options.meta
  return out
}

/** Convenience wrapper for child layout metadata. */
export function withLayout(node: ComponentSpec, layout: LayoutSpec, meta?: ComponentProps): ComponentSpecChild {
  return child(node, { layout, meta })
}

/** Convenience wrapper for child meta values. */
export function withMeta(node: ComponentSpec, meta: ComponentProps): ComponentSpecChild {
  return child(node, { meta })
}

/** Create a TabView child with the title expected by the runtime TabView builder. */
export function tab(title: string, node: ComponentSpec, layout?: LayoutSpec): ComponentSpecChild {
  return child(node, { layout, meta: { title } })
}

export function Text(text: string, options: TextOptions = {}): ComponentSpec {
  return makeSpec('Text', options.id, {
    text,
    selectable: options.selectable,
    clipboard: options.clipboard,
  }, options.events)
}

export function Label(text: string, options: LabelOptions = {}): ComponentSpec {
  return makeSpec('Label', options.id, {
    text,
    enabled: enabledValue(options),
  }, options.events)
}

export function Button(label: string, options?: Omit<ButtonOptions, 'label' | 'text'>): ComponentSpec
export function Button(options?: ButtonOptions): ComponentSpec
export function Button(
  first: string | ButtonOptions = {},
  second: Omit<ButtonOptions, 'label' | 'text'> = {},
): ComponentSpec {
  const options: ButtonOptions = typeof first === 'string' ? { ...second, label: first } : first
  return makeSpec('Button', options.id, {
    label: options.label ?? options.text,
    enabled: enabledValue(options),
  }, events(options.events, { click: options.onClick }))
}

export function TextBox(options: TextBoxOptions = {}): ComponentSpec {
  return makeSpec('TextBox', options.id, {
    title: options.title,
    text: options.text,
    border: options.border,
    placeholder: options.placeholder,
    clipboard: options.clipboard,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange, submit: options.onSubmit }))
}

export function TextArea(options: TextAreaOptions = {}): ComponentSpec {
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

export function Checkbox(options: CheckboxOptions = {}): ComponentSpec {
  return makeSpec('Checkbox', options.id, {
    label: options.label,
    checked: options.checked,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange }))
}

export function RadioGroup(options: ChoiceOptions = {}): ComponentSpec {
  return makeSpec('RadioGroup', options.id, {
    label: options.label,
    options: options.options,
    selection: options.selection,
    height: options.height,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange }))
}

export function Slider(options: SliderOptions = {}): ComponentSpec {
  return makeSpec('Slider', options.id, {
    min: options.min,
    max: options.max,
    value: options.value,
    step: options.step,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange }))
}

export function Spinner(textOrOptions: string | SpinnerOptions = {}, options: SpinnerOptions = {}): ComponentSpec {
  const props = typeof textOrOptions === 'string' ? { ...options, text: textOrOptions } : textOrOptions
  return makeSpec('Spinner', props.id, {
    text: props.text,
    running: props.running,
    enabled: enabledValue(props),
  }, props.events)
}

export function ProgressBar(options: ProgressBarOptions = {}): ComponentSpec {
  return makeSpec('ProgressBar', options.id, {
    min: options.min,
    max: options.max,
    value: options.value,
    show_text: options.showText,
    text: options.text,
    enabled: enabledValue(options),
  }, options.events)
}

export function ListBox(options: ChoiceOptions = {}): ComponentSpec {
  return makeSpec('ListBox', options.id, {
    title: options.title,
    items: options.items,
    selection: options.selection,
    height: options.height,
    border: options.border,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange }))
}

export function TableView(options: TableViewOptions = {}): ComponentSpec {
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

export function VStack(children?: ComponentChildren): ComponentSpec
export function VStack(options?: StackOptions | null, children?: ComponentChildren): ComponentSpec
export function VStack(first: StackOptions | ComponentChildren | null = {}, second?: ComponentChildren): ComponentSpec {
  const { options, children } = containerArgs(first, second)
  return makeSpec('VStack', options.id, stackProps(options), options.events, children)
}

export function HStack(children?: ComponentChildren): ComponentSpec
export function HStack(options?: StackOptions | null, children?: ComponentChildren): ComponentSpec
export function HStack(first: StackOptions | ComponentChildren | null = {}, second?: ComponentChildren): ComponentSpec {
  const { options, children } = containerArgs(first, second)
  return makeSpec('HStack', options.id, stackProps(options), options.events, children)
}

export function Grid(children?: ComponentChildren): ComponentSpec
export function Grid(options?: GridOptions | null, children?: ComponentChildren): ComponentSpec
export function Grid(first: GridOptions | ComponentChildren | null = {}, second?: ComponentChildren): ComponentSpec {
  const { options, children } = containerArgs(first, second)
  return makeSpec('Grid', options.id, {
    columns: options.columns,
    row_gap: options.rowGap,
    column_gap: options.columnGap,
    padding: options.padding,
    scrollable: options.scrollable,
  }, options.events, children)
}

export function Border(node: ComponentSpec, options: BorderOptions = {}): ComponentSpec {
  return makeSpec('Border', options.id, { border: options.border }, options.events, [node])
}

export function Visibility(node: ComponentSpec, options: VisibilityOptions = {}): ComponentSpec {
  return makeSpec('Visibility', options.id, { visible: options.visible }, options.events, [node])
}

export function Divider(orientationOrOptions: string | DividerOptions = {}, options: DividerOptions = {}): ComponentSpec {
  const props = typeof orientationOrOptions === 'string' ? { ...options, orientation: orientationOrOptions } : orientationOrOptions
  return makeSpec('Divider', props.id, { orientation: props.orientation }, props.events)
}

export function Spacer(options: SpacerOptions = {}): ComponentSpec {
  return makeSpec('Spacer', options.id, undefined, options.events)
}

export function Splitter(first: ComponentSpec, second: ComponentSpec, options: SplitterOptions = {}): ComponentSpec {
  return makeSpec('Splitter', options.id, {
    orientation: options.orientation,
    split_pos: options.splitPos,
    min_first: options.minFirst,
    min_second: options.minSecond,
    border: options.border,
  }, options.events, [first, second])
}

export function TabView(children?: ComponentChildren): ComponentSpec
export function TabView(options?: TabViewOptions | null, children?: ComponentChildren): ComponentSpec
export function TabView(first: TabViewOptions | ComponentChildren | null = {}, second?: ComponentChildren): ComponentSpec {
  const { options, children } = containerArgs(first, second)
  return makeSpec('TabView', options.id, {
    selection: options.selection,
    header_position: options.headerPosition,
  }, events(options.events, { change: options.onChange }), children)
}

export function TextSpan(text: string, options: TextSpanOptions = {}): ComponentSpec {
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

export function RichText(children?: ComponentChildren): ComponentSpec
export function RichText(options?: RichTextOptions | null, children?: ComponentChildren): ComponentSpec
export function RichText(first: RichTextOptions | ComponentChildren | null = {}, second?: ComponentChildren): ComponentSpec {
  const { options, children } = containerArgs(first, second)
  return makeSpec('RichText', options.id, undefined, events(options.events, { link: options.onLink }), children)
}

export function StyledLabel(text: string, options: StyledLabelOptions = {}): ComponentSpec {
  return makeSpec('StyledLabel', options.id, {
    text,
    enabled: enabledValue(options),
  }, events(options.events, { link: options.onLink }))
}

export function Disclosure(options: DisclosureOptions = {}, children?: ComponentChildren): ComponentSpec {
  return makeSpec('Disclosure', options.id, {
    title: options.title,
    content: options.content,
    expanded: options.expanded,
    status: options.status,
    enabled: enabledValue(options),
  }, events(options.events, { toggle: options.onToggle }), children ?? options.children)
}

export function TypeAhead(options: TypeAheadOptions = {}): ComponentSpec {
  return makeSpec('TypeAhead', options.id, typeAheadProps(options), typeAheadEvents(options))
}

export function CommandPalette(options: CommandPaletteOptions = {}): ComponentSpec {
  return makeSpec('CommandPalette', options.id, typeAheadProps(options), typeAheadEvents(options))
}

export function MarkdownViewer(markdown: string, options?: Omit<MarkdownViewerOptions, 'markdown' | 'text'>): ComponentSpec
export function MarkdownViewer(options?: MarkdownViewerOptions): ComponentSpec
export function MarkdownViewer(
  first: string | MarkdownViewerOptions = {},
  second: Omit<MarkdownViewerOptions, 'markdown' | 'text'> = {},
): ComponentSpec {
  const options: MarkdownViewerOptions = typeof first === 'string' ? { ...second, markdown: first } : first
  return makeSpec('MarkdownViewer', options.id, {
    markdown: options.markdown ?? options.text,
    wrap_width: options.wrapWidth,
    show_markers: options.showMarkers,
    vertical_scrollbar: options.verticalScrollbar,
    code_block_max_height: options.codeBlockMaxHeight,
    table_max_height: options.tableMaxHeight,
  }, events(options.events, { link: options.onLink }))
}

export function TerminalEmulator(options: TerminalEmulatorOptions = {}): ComponentSpec {
  return makeSpec('TerminalEmulator', options.id, {
    command: options.command,
    args: options.args,
    scrollback_len: options.scrollbackLen,
    capture: options.capture,
    capture_on_click: options.captureOnClick,
    scroll_step: options.scrollStep,
  }, events(options.events, { input: options.onInput, close: options.onClose }))
}

export function FileTreeNode(id: number, name: string, options: FileTreeNodeOptions = {}): ComponentValueMap {
  return fileTreeNodeValue({ ...options, id, name })
}

export function FileTree(options: FileTreeOptions = {}): ComponentSpec {
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

function fileTreeIconsValue(
  icons: Readonly<Record<string, FileTreeIconLike>> | undefined,
): ComponentValueMap | undefined {
  if (icons === undefined) return undefined
  const out: Record<string, ComponentValue> = {}
  for (const [ext, icon] of Object.entries(icons)) {
    out[ext] = typeof icon === 'string' ? icon : { glyph: icon.glyph, ...(icon.color !== undefined ? { color: icon.color } : {}) }
  }
  return out
}

export function ChatMessage(
  messageId: number,
  blocks: readonly ChatBlockInput[],
  options: ChatMessageBaseOptions = {},
): ChatMessageInput {
  const meta = chatMetaValue(options)
  const message = compactRecord({
    id: messageId,
    role: options.role ?? 'assistant',
    status: options.status ?? 'complete',
    meta,
    blocks,
  })
  return message as unknown as ChatMessageInput
}

export function ChatTextBlock(blockId: number, markdown: string, options: ChatTextBlockOptions = {}): ChatTextBlock {
  return compactRecord({
    type: 'text',
    block_id: blockId,
    markdown,
    streaming: options.streaming,
  }) as unknown as ChatTextBlock
}

export function ChatThinkingBlock(
  blockId: number,
  markdown: string,
  options: ChatThinkingBlockOptions = {},
): ChatThinkingBlock {
  return compactRecord({
    type: 'thinking',
    block_id: blockId,
    markdown,
    streaming: options.streaming,
    collapsed: options.collapsed,
  }) as unknown as ChatThinkingBlock
}

export function ChatToolTextInput(text: string): ChatToolTextInput {
  return { text }
}

export function ChatToolJsonInput(json: ComponentValue): ChatToolJsonInput {
  return { json }
}

export function ChatToolAnsiOutput(ansi: string): ChatToolAnsiOutput {
  return { ansi }
}

export function ChatToolMarkdownOutput(markdown: string): ChatToolMarkdownOutput {
  return { markdown }
}

export function ChatToolDiffOutput(diff: string): ChatToolDiffOutput {
  return { diff }
}

export function ChatToolUseBlock(
  blockId: number,
  callId: string,
  name: string,
  options: ChatToolUseBlockOptions = {},
): ChatToolUseBlock {
  return compactRecord({
    type: 'tool_use',
    block_id: blockId,
    call_id: callId,
    name,
    input: options.input ?? ChatToolTextInput(''),
    status: options.status ?? 'pending',
    approval: options.approval,
    collapsed: options.collapsed,
  }) as unknown as ChatToolUseBlock
}

export function ChatToolResultBlock(
  blockId: number,
  callId: string,
  options: ChatToolResultBlockOptions = {},
): ChatToolResultBlock {
  return compactRecord({
    type: 'tool_result',
    block_id: blockId,
    call_id: callId,
    ok: options.ok ?? true,
    exit_code: options.exitCode,
    output: options.output ?? ChatToolAnsiOutput(''),
    collapsed: options.collapsed,
  }) as unknown as ChatToolResultBlock
}

export function ChatDiffBlock(
  blockId: number,
  path: string,
  diff: string,
  options: ChatDiffBlockOptions = {},
): ChatDiffBlock {
  return compactRecord({
    type: 'diff',
    block_id: blockId,
    path,
    diff,
    decision: options.decision ?? 'pending',
  }) as unknown as ChatDiffBlock
}

export function ChatPlanBlock(
  blockId: number,
  items: readonly ChatPlanItem[],
  options: ChatPlanBlockOptions = {},
): ChatPlanBlock {
  return compactRecord({
    type: 'plan',
    block_id: blockId,
    items,
    decision: options.decision ?? 'pending',
  }) as unknown as ChatPlanBlock
}

export function ChatTaskTranscriptItem(
  role: ChatRole,
  blocks: readonly ChatBlockInput[],
): ChatTaskTranscriptItem {
  return { role, blocks }
}

export function ChatTaskBlock(
  blockId: number,
  title: string,
  options: ChatTaskBlockOptions = {},
): ChatTaskBlock {
  return compactRecord({
    type: 'task',
    block_id: blockId,
    title,
    status: options.status ?? 'pending',
    summary: options.summary ?? '',
    transcript: options.transcript ?? [],
    collapsed: options.collapsed,
  }) as unknown as ChatTaskBlock
}

export function ChatTodoBlock(blockId: number, items: readonly ChatTodoItem[]): ChatTodoBlock {
  return compactRecord({ type: 'todo', block_id: blockId, items }) as unknown as ChatTodoBlock
}

export function ChatAttachmentBlock(
  blockId: number,
  name: string,
  options: Pick<ChatAttachmentBlock, 'url' | 'mime'> = {},
): ChatAttachmentBlock {
  return compactRecord({
    type: 'attachment',
    block_id: blockId,
    name,
    url: options.url,
    mime: options.mime,
  }) as unknown as ChatAttachmentBlock
}

export function ChatNoticeBlock(blockId: number, level: ChatNoticeLevel, text: string): ChatNoticeBlock {
  return compactRecord({ type: 'notice', block_id: blockId, level, text }) as unknown as ChatNoticeBlock
}

export function ChatArtifactBlock(
  blockId: number,
  options: Omit<ChatArtifactBlock, 'type' | 'block_id'>,
): ChatArtifactBlock {
  return compactRecord({
    type: 'artifact',
    block_id: blockId,
    kind: options.kind,
    anchor: options.anchor,
    title: options.title,
  }) as unknown as ChatArtifactBlock
}

export function ChatTextMessage(
  messageId: number,
  markdown: string,
  options: ChatTextMessageOptions = {},
): ChatMessageInput {
  return ChatMessage(messageId, [
    ChatTextBlock(options.blockId ?? derivedChatBlockId(messageId, 0), markdown, options),
  ], options)
}

export function ChatFileMessage(
  messageId: number,
  name: string,
  options: ChatFileMessageOptions = {},
): ChatMessageInput {
  return ChatMessage(messageId, [
    ChatAttachmentBlock(options.blockId ?? derivedChatBlockId(messageId, 0), name, options),
  ], options)
}

export function ChatToolCallMessage(
  messageId: number,
  name: string,
  options: ChatToolCallMessageOptions = {},
): ChatMessageInput {
  const callId = options.callId ?? `tool-${messageId}`
  const toolStatus = options.toolStatus ?? 'running'
  const turnStatus = toolStatus === 'pending' || toolStatus === 'running' ? 'streaming' : 'complete'
  const blocks: ChatBlockInput[] = [
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

export function ChatArtifactMessage(
  messageId: number,
  options: ChatArtifactMessageOptions,
): ChatMessageInput {
  return ChatMessage(messageId, [
    ChatArtifactBlock(options.blockId ?? derivedChatBlockId(messageId, 0), options),
  ], options)
}

export function ChatMessageList(options: ChatMessageListOptions = {}): ComponentSpec {
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

export function ChatInputMode(mode = 'text', options: ChatInputModeOptions = {}): ComponentValueMap {
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

export function ChatSlashCommand(
  label: string,
  options: ChatSlashCommandOptions = {},
): ChatSlashCommandInput {
  return compactRecord({
    id: options.id,
    label,
    detail: options.detail,
    replacement: options.replacement,
    action: options.action,
  }) as unknown as ChatSlashCommandInput
}

export function ChatMentionCandidate(
  label: string,
  options: ChatMentionCandidateOptions = {},
): ChatMentionCandidateInput {
  return compactRecord({
    id: options.id,
    label,
    detail: options.detail,
    replacement: options.replacement,
  }) as unknown as ChatMentionCandidateInput
}

export function ChatInputPanel(options: ChatInputPanelOptions = {}): ComponentSpec {
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

function makeSpec(
  type: string,
  id?: string,
  props?: PropInput,
  eventInput?: EventInput,
  children?: ComponentChildren,
): ComponentSpec {
  const spec: { type: string; id?: string; props?: ComponentProps; events?: ComponentEvents; children?: ComponentChildren } = { type }
  const compactProps = compactRecord(props)
  const compactEvents = compactEventRecord(eventInput)
  if (id !== undefined) spec.id = id
  if (compactProps !== undefined) spec.props = compactProps
  if (compactEvents !== undefined) spec.events = compactEvents
  if (children !== undefined && children.length > 0) spec.children = children
  return spec
}

function compactRecord(record: PropInput | undefined): ComponentProps | undefined {
  if (record === undefined) return undefined
  const out: Record<string, ComponentValue> = {}
  for (const [key, value] of Object.entries(record)) {
    if (value !== undefined) out[key] = value as ComponentValue
  }
  return Object.keys(out).length > 0 ? out : undefined
}

function compactEventRecord(record: EventInput | undefined): ComponentEvents | undefined {
  if (record === undefined) return undefined
  const out: Record<string, string> = {}
  for (const [key, value] of Object.entries(record)) {
    if (value !== undefined) out[key] = value
  }
  return Object.keys(out).length > 0 ? out : undefined
}

function events(base: EventInput | undefined, aliases: EventInput): ComponentEvents | undefined {
  const merged: Record<string, CallbackHandle | undefined> = { ...(base ?? {}) }
  for (const [key, value] of Object.entries(aliases)) {
    if (value !== undefined) merged[key] = value
  }
  return compactEventRecord(merged)
}

function enabledValue(options: EnabledOptions): boolean | undefined {
  if (options.enabled !== undefined) return options.enabled
  if (options.disabled !== undefined) return !options.disabled
  return undefined
}

function containerArgs<T extends ContainerOptions>(
  first: T | ComponentChildren | null,
  second: ComponentChildren | undefined,
): { options: T; children: ComponentChildren | undefined } {
  if (Array.isArray(first)) {
    return { options: {} as T, children: first }
  }
  const options = (first ?? {}) as T
  return { options, children: second ?? options.children }
}

function stackProps(options: StackOptions): PropInput {
  return {
    spacing: options.spacing,
    padding: options.padding,
    scrollable: options.scrollable,
  }
}

function typeAheadProps(options: TypeAheadOptions): PropInput {
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

function typeAheadEvents(options: TypeAheadOptions): ComponentEvents | undefined {
  return events(options.events, {
    change: options.onChange,
    accept: options.onAccept,
    close: options.onClose,
  })
}

/**
 * Normalizes a file-tree node input into the runtime node value map (mapping
 * `isExpanded`→`expanded`, `nodes`→`children`, recursively). A node that is
 * already a `ComponentValueMap` is returned unchanged.
 */
export function fileTreeNodeValue(node: FileTreeNodeLike): ComponentValueMap {
  if (!isFileTreeNodeInput(node)) return node
  const children = node.children ?? node.nodes
  return compactRecord({
    id: node.id,
    name: node.name,
    kind: node.kind,
    children: children?.map(fileTreeNodeValue),
    expanded: node.expanded ?? node.isExpanded,
  }) ?? {}
}

function isFileTreeNodeInput(node: FileTreeNodeLike): node is FileTreeNodeInput {
  return typeof node.id === 'number' && typeof node.name === 'string'
}

function chatMetaValue(options: ChatMessageBaseOptions): ChatMessageMeta | undefined {
  const meta = options.meta
  const out = compactRecord({
    timestamp: options.timestamp !== undefined ? options.timestamp : meta?.timestamp,
    model: meta?.model,
    usage: meta?.usage,
    elapsed_ms: meta?.elapsed_ms,
    stop_reason: meta?.stop_reason,
  })
  return out as unknown as ChatMessageMeta | undefined
}

function derivedChatBlockId(messageId: number, ordinal: number): number {
  return messageId * 1_000 + ordinal + 1
}

function chatToolOutputFromString(kind: ChatToolOutputKind, output: string): ChatToolOutput {
  switch (kind) {
    case 'ansi':
      return ChatToolAnsiOutput(output)
    case 'markdown':
      return ChatToolMarkdownOutput(output)
    case 'diff':
      return ChatToolDiffOutput(output)
  }
}

function normalizeName(name: string): string {
  return Array.from(name)
    .filter((char) => char !== '_' && char !== '-' && char !== ' ')
    .join('')
    .toLowerCase()
}

function isEmptyRecord(record: ComponentProps | undefined): boolean {
  return record === undefined || Object.keys(record).length === 0
}
