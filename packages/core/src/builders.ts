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
export type ChatSender = 'user' | 'assistant' | 'system' | `tool:${string}` | `custom:${string}` | (string & {})
export type ChatMessageStatus = 'final' | 'in_progress' | 'inprogress' | (string & {})
export type ChatToolCallStatus = 'running' | 'done' | 'error' | (string & {})
export type ChatArtifactKind = 'code' | 'diff' | 'file' | (string & {})

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
  readonly sender?: ChatSender
  readonly status?: ChatMessageStatus
  readonly timestamp?: string | null
}

export interface ChatTextMessageOptions extends ChatMessageBaseOptions {}

export interface ChatFileMessageOptions extends ChatMessageBaseOptions {
  readonly url?: string | null
}

export interface ChatToolCallMessageOptions extends ChatMessageBaseOptions {
  readonly output?: string
  readonly toolStatus?: ChatToolCallStatus
}

export interface ChatArtifactMessageOptions extends ChatMessageBaseOptions {
  readonly kind: ChatArtifactKind
  readonly anchor: string | number
  readonly title: string
}

export type ChatMessageInput = ComponentValueMap

export interface ChatMessageListOptions extends BuilderBaseOptions {
  readonly messages?: readonly ChatMessageInput[]
  readonly spacing?: number
  readonly padding?: EdgeInsetsSpec
  readonly wrapWidth?: number
  readonly showTimestamps?: boolean
  readonly autoScroll?: boolean
  readonly onLoadMore?: CallbackHandle
  readonly onOpenArtifact?: CallbackHandle
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

export interface ChatInputPanelOptions extends BuilderBaseOptions, EnabledOptions {
  readonly mode?: ChatInputModeInput
  readonly draft?: string
  readonly custom?: string
  readonly history?: readonly string[]
  readonly selection?: number
  readonly clearOnSubmit?: boolean
  readonly onSubmit?: CallbackHandle
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

export function ChatTextMessage(
  messageId: number,
  markdown: string,
  options: ChatTextMessageOptions = {},
): ComponentValueMap {
  return chatMessage(messageId, { markdown }, options)
}

export function ChatFileMessage(
  messageId: number,
  name: string,
  options: ChatFileMessageOptions = {},
): ComponentValueMap {
  return chatMessage(messageId, { file: { name, url: options.url ?? null } }, options)
}

export function ChatToolCallMessage(
  messageId: number,
  name: string,
  options: ChatToolCallMessageOptions = {},
): ComponentValueMap {
  return chatMessage(messageId, {
    tool_call: {
      name,
      status: options.toolStatus ?? 'running',
      output: options.output ?? '',
    },
  }, {
    ...options,
    status: options.status ?? 'in_progress',
  })
}

export function ChatArtifactMessage(
  messageId: number,
  options: ChatArtifactMessageOptions,
): ComponentValueMap {
  return chatMessage(messageId, {
    artifact: {
      kind: options.kind,
      anchor: options.anchor,
      title: options.title,
    },
  }, options)
}

export function ChatMessageList(options: ChatMessageListOptions = {}): ComponentSpec {
  return makeSpec('ChatMessageList', options.id, {
    messages: options.messages,
    spacing: options.spacing,
    padding: options.padding,
    wrap_width: options.wrapWidth,
    show_timestamps: options.showTimestamps,
    auto_scroll: options.autoScroll,
  }, events(options.events, {
    load_more: options.onLoadMore,
    open_artifact: options.onOpenArtifact,
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

export function ChatInputPanel(options: ChatInputPanelOptions = {}): ComponentSpec {
  return makeSpec('ChatInputPanel', options.id, {
    mode: options.mode ?? ChatInputMode(),
    draft: options.draft,
    custom: options.custom,
    history: options.history,
    selection: options.selection,
    enabled: enabledValue(options),
    clear_on_submit: options.clearOnSubmit,
  }, events(options.events, { submit: options.onSubmit }))
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

function chatMessage(
  messageId: number,
  content: ComponentValueMap,
  options: ChatMessageBaseOptions,
): ComponentValueMap {
  return compactRecord({
    id: messageId,
    sender: options.sender ?? 'assistant',
    timestamp: options.timestamp ?? null,
    status: options.status ?? 'final',
    content,
  }) ?? {}
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
