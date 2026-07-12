import { createElement, type ReactElement, type ReactNode } from 'react'
import {
  ChatInputMode as chatInputModeValue,
  fileTreeNodeValue,
  type ChatMentionCandidateInput,
  type ChatMentionContext,
  type ChatInputModeInput,
  type ChatInputModeOptions,
  type ChatMessageInput,
  type ChatSlashCommandAction,
  type ChatSlashCommandInput,
  type EdgeInsetsSpec,
  type FileTreeIconLike,
  type FileTreeNodeLike,
  type LayoutSpec,
} from '@atto-ui/core'

import type { AttoUiCallbackEvent, AttoUiEventHandler } from './events'

export type ValueChangeHandler<T> = (value: T, event: AttoUiCallbackEvent) => void

export type PrimitiveLabel = string | number

/**
 * Per-child layout applied when a component is placed inside a stack or grid.
 * For example `layout={{ height: 'fill' }}` lets a control flex to share the
 * remaining space instead of packing to its intrinsic height. `width`/`height`
 * accept `'fill'`, `'content'`, a number (weight), or `{ fixed }`/`{ weight }`.
 */
export interface LayoutProps {
  readonly layout?: LayoutSpec
}

export interface LabelProps extends LayoutProps {
  readonly text: string
  readonly enabled?: boolean
}

export interface ButtonHostProps {
  readonly label?: string
  readonly enabled?: boolean
  readonly onClick?: AttoUiEventHandler
}

export interface ButtonProps extends ButtonHostProps, LayoutProps {
  readonly children?: PrimitiveLabel
}

export interface TextBoxHostProps {
  readonly title?: string
  readonly text?: string
  readonly placeholder?: string
  readonly enabled?: boolean
  /** Draw the widget's own border. Defaults to `true`. */
  readonly border?: boolean
  readonly clipboard?: string
  readonly onChange?: AttoUiEventHandler
  readonly onSubmit?: AttoUiEventHandler
}

export interface TextBoxProps extends Omit<TextBoxHostProps, 'text' | 'onChange'>, LayoutProps {
  readonly value: string
  readonly onChange?: ValueChangeHandler<string>
}

export interface ListBoxHostProps {
  readonly title?: string
  readonly items?: readonly string[]
  readonly selection?: number
  readonly height?: number
  readonly enabled?: boolean
  /** Draw the widget's own border. Defaults to `true`. */
  readonly border?: boolean
  readonly onChange?: AttoUiEventHandler
}

export interface ListBoxProps extends Omit<ListBoxHostProps, 'onChange'>, LayoutProps {
  readonly items: readonly string[]
  readonly selectedIndex?: number
  readonly onChange?: ValueChangeHandler<number>
  readonly onSelect?: ValueChangeHandler<number>
}

export interface TableViewHostProps {
  readonly title?: string
  readonly headers?: readonly string[]
  readonly rows?: readonly (readonly string[])[]
  readonly selection?: number
  readonly height?: number
  readonly enabled?: boolean
  /** Draw the widget's own border. Defaults to `true`. */
  readonly border?: boolean
  readonly onChange?: AttoUiEventHandler
}

export interface TableProps extends Omit<TableViewHostProps, 'onChange'>, LayoutProps {
  readonly rows: readonly (readonly string[])[]
  readonly selectedIndex?: number
  readonly onChange?: ValueChangeHandler<number>
  readonly onSelect?: ValueChangeHandler<number>
}

export interface StackProps extends LayoutProps {
  readonly spacing?: number
  readonly padding?: EdgeInsetsSpec
  readonly scrollable?: boolean
  readonly children?: ReactNode
}

export interface GridProps extends LayoutProps {
  readonly columns?: number
  readonly rowGap?: number
  readonly columnGap?: number
  readonly padding?: EdgeInsetsSpec
  readonly scrollable?: boolean
  readonly children?: ReactNode
}

/** Typed React wrapper for the native Button host component. */
export function Button({ label, children, enabled, onClick, layout }: ButtonProps): ReactElement {
  return hostElement('button', {
    label: label ?? childrenLabel(children) ?? 'Button',
    enabled,
    onClick,
    layout,
  })
}

/** Controlled single-line TextBox wrapper using the runtime `text` property. */
export function TextBox(props: TextBoxProps): ReactElement {
  const { title, value, placeholder, enabled, border, clipboard, onChange, onSubmit, layout } = props
  return hostElement('textBox', {
    __attoControlledText: true,
    title,
    text: value,
    placeholder,
    enabled,
    border,
    clipboard,
    onChange: onChange === undefined ? undefined : controlledTextChange('TextBox', onChange),
    onSubmit,
    layout,
  })
}

/** Typed ListBox wrapper; `onSelect` and `onChange` receive the selected index. */
export function ListBox(props: ListBoxProps): ReactElement {
  const { title, items, selection, selectedIndex, height, enabled, border, onChange, onSelect, layout } = props
  return hostElement('listBox', {
    title,
    items,
    selection: selectedIndex ?? selection ?? 0,
    height,
    enabled,
    border,
    onChange: selectionHandler('ListBox', onChange, onSelect),
    layout,
  })
}

/** Typed TableView wrapper with the shorter exported name `Table`. */
export function Table(props: TableProps): ReactElement {
  const { title, headers, rows, selection, selectedIndex, height, enabled, border, onChange, onSelect, layout } = props
  return hostElement('tableView', {
    title,
    headers,
    rows,
    selection: selectedIndex ?? selection ?? 0,
    height,
    enabled,
    border,
    onChange: selectionHandler('Table', onChange, onSelect),
    layout,
  })
}

export const TableView = Table

/** Payload delivered to `onRename` when a file-tree entry is renamed. */
export interface FileTreeRenamePayload {
  readonly id: number
  readonly kind: string
  readonly oldName: string
  readonly newName: string
}

/** Payload delivered to `onDelete` when a file-tree entry is removed. */
export interface FileTreeDeletePayload {
  readonly id: number
  readonly kind: string
  readonly name: string
}

export type FileTreeSelectHandler = (nodeId: number | null, event: AttoUiCallbackEvent) => void
export type FileTreeRenameHandler = (payload: FileTreeRenamePayload, event: AttoUiCallbackEvent) => void
export type FileTreeDeleteHandler = (payload: FileTreeDeletePayload, event: AttoUiCallbackEvent) => void

export interface FileTreeHostProps {
  readonly title?: string
  readonly nodes?: readonly FileTreeNodeLike[]
  /** Selected node id, or `null` for no selection. */
  readonly selection?: number | null
  readonly height?: number
  readonly enabled?: boolean
  /** Draw the widget's own border. Defaults to `true`. */
  readonly border?: boolean
  /** Map of lowercased file extension → icon (string or `{glyph,color}`). */
  readonly icons?: Readonly<Record<string, FileTreeIconLike>>
  readonly onSelect?: AttoUiEventHandler
  readonly onRename?: AttoUiEventHandler
  readonly onDelete?: AttoUiEventHandler
}

export interface FileTreeProps
  extends Omit<FileTreeHostProps, 'onSelect' | 'onRename' | 'onDelete'>,
    LayoutProps {
  readonly onSelect?: FileTreeSelectHandler
  readonly onRename?: FileTreeRenameHandler
  readonly onDelete?: FileTreeDeleteHandler
}

export interface ChatMessageListHostProps {
  readonly messages?: readonly ChatMessageInput[]
  readonly spacing?: number
  readonly padding?: EdgeInsetsSpec
  readonly wrap_width?: number
  readonly show_timestamps?: boolean
  readonly bubble_width_percent?: number
  readonly auto_scroll?: boolean
  readonly onLoad_more?: AttoUiEventHandler
  readonly onOpen_artifact?: AttoUiEventHandler
  readonly onApprove?: AttoUiEventHandler
  readonly onEdit_decision?: AttoUiEventHandler
  readonly onPlan_decision?: AttoUiEventHandler
  readonly onCancel?: AttoUiEventHandler
  readonly onMessage_action?: AttoUiEventHandler
}

export interface ChatMessageListProps extends LayoutProps {
  readonly messages?: readonly ChatMessageInput[]
  readonly spacing?: number
  readonly padding?: EdgeInsetsSpec
  readonly wrapWidth?: number
  readonly showTimestamps?: boolean
  /** Percent (1..=100) of list width a bubble may occupy. Default 75. */
  readonly bubbleWidthPercent?: number
  /** Convenience for `bubbleWidthPercent={100}` — messages span the full list width. */
  readonly fillWidth?: boolean
  readonly autoScroll?: boolean
  readonly onLoadMore?: AttoUiEventHandler
  readonly onOpenArtifact?: AttoUiEventHandler
  /** Fired when an inline tool-approval option is chosen. Payload is a map. */
  readonly onApprove?: AttoUiEventHandler
  /** Fired when an inline diff is accepted/rejected. Payload is a map. */
  readonly onEditDecision?: AttoUiEventHandler
  readonly onPlanDecision?: AttoUiEventHandler
  /** Fired when a streaming turn is cancelled. Payload is a map. */
  readonly onCancel?: AttoUiEventHandler
  /** Fired for copy/retry/regenerate/edit/copy-block actions. Payload is a map. */
  readonly onMessageAction?: AttoUiEventHandler
}

/** Typed FileTree wrapper. Pass `nodes` as plain node inputs or core node maps. */
export function FileTree(props: FileTreeProps): ReactElement {
  const { title, nodes, selection, height, enabled, border, icons, onSelect, onRename, onDelete, layout } = props
  return hostElement('fileTree', {
    title,
    nodes: nodes?.map(fileTreeNodeValue),
    selection,
    height,
    enabled,
    border,
    icons,
    onSelect: fileTreeSelectHandler(onSelect),
    onRename: fileTreeRenameHandler(onRename),
    onDelete: fileTreeDeleteHandler(onDelete),
    layout,
  })
}

/** Chat transcript wrapper using the block-based runtime message shape. */
export function ChatMessageList(props: ChatMessageListProps): ReactElement {
  const {
    messages,
    spacing,
    padding,
    wrapWidth,
    showTimestamps,
    bubbleWidthPercent,
    fillWidth,
    autoScroll,
    onLoadMore,
    onOpenArtifact,
    onApprove,
    onEditDecision,
    onPlanDecision,
    onCancel,
    onMessageAction,
    layout,
  } = props
  return hostElement('chatMessageList', {
    messages,
    spacing,
    padding,
    wrap_width: wrapWidth,
    show_timestamps: showTimestamps,
    bubble_width_percent: fillWidth ? 100 : bubbleWidthPercent,
    auto_scroll: autoScroll,
    onLoad_more: onLoadMore,
    onOpen_artifact: onOpenArtifact,
    onApprove,
    onEdit_decision: onEditDecision,
    onPlan_decision: onPlanDecision,
    onCancel,
    onMessage_action: onMessageAction,
    layout,
  })
}

/** Input mode kind for {@link ChatInputPanel}. */
export type ChatInputKind = 'text' | 'choice' | 'confirm'

/** Friendly input-mode descriptor; converted to the runtime mode map via core's `ChatInputMode`. */
export interface ChatInputModeSpec extends ChatInputModeOptions {
  /** Defaults to `'text'`. */
  readonly kind?: ChatInputKind
}

export interface ChatInputPanelHostProps {
  readonly mode?: ChatInputModeInput
  readonly draft?: string
  readonly custom?: string
  readonly history?: readonly string[]
  readonly slash_commands?: readonly ChatSlashCommandInput[]
  readonly mention_candidates?: readonly ChatMentionCandidateInput[]
  readonly selection?: number
  readonly enabled?: boolean
  readonly clear_on_submit?: boolean
  readonly onSubmit?: AttoUiEventHandler
  readonly onSlash_command?: AttoUiEventHandler
  readonly onMention_query?: AttoUiEventHandler
}

export type ChatSlashCommandHandler = (
  command: ChatSlashCommandInput,
  event: AttoUiCallbackEvent,
) => void

export type ChatMentionQueryHandler = (
  context: ChatMentionContext,
  event: AttoUiCallbackEvent,
) => void

export interface ChatInputPanelProps extends LayoutProps {
  /**
   * Either a friendly {@link ChatInputModeSpec} (recommended) or an already-built
   * runtime mode map produced by core's `ChatInputMode()`.
   */
  readonly mode?: ChatInputModeSpec | ChatInputModeInput
  readonly draft?: string
  readonly custom?: string
  readonly history?: readonly string[]
  readonly slashCommands?: readonly ChatSlashCommandInput[]
  readonly mentionCandidates?: readonly ChatMentionCandidateInput[]
  readonly selection?: number
  readonly enabled?: boolean
  readonly clearOnSubmit?: boolean
  /** Fired when the user submits. Payload is a map (text / choice / custom). */
  readonly onSubmit?: AttoUiEventHandler
  /** Fired when a submit-action slash command is accepted. */
  readonly onSlashCommand?: ChatSlashCommandHandler
  /** Fired when an `@` mention query changes; update `mentionCandidates` in response. */
  readonly onMentionQuery?: ChatMentionQueryHandler
}

function resolveInputMode(
  mode: ChatInputModeSpec | ChatInputModeInput | undefined,
): ChatInputModeInput | undefined {
  if (!mode) return undefined
  // An already-built mode map carries a `type` discriminant; a friendly spec does not.
  if (typeof (mode as { type?: unknown }).type === 'string') {
    return mode as ChatInputModeInput
  }
  const { kind, ...options } = mode as ChatInputModeSpec
  return chatInputModeValue(kind ?? 'text', options)
}

/** Chat input panel wrapper supporting text / choice / confirm modes. */
export function ChatInputPanel(props: ChatInputPanelProps): ReactElement {
  const {
    mode,
    draft,
    custom,
    history,
    slashCommands,
    mentionCandidates,
    selection,
    enabled,
    clearOnSubmit,
    onSubmit,
    onSlashCommand,
    onMentionQuery,
    layout,
  } = props
  return hostElement('chatInputPanel', {
    mode: resolveInputMode(mode),
    draft,
    custom,
    history,
    slash_commands: slashCommands,
    mention_candidates: mentionCandidates,
    selection,
    enabled,
    clear_on_submit: clearOnSubmit,
    onSubmit,
    onSlash_command: slashCommandHandler(onSlashCommand),
    onMention_query: mentionQueryHandler(onMentionQuery),
    layout,
  })
}

export interface ChatPanelProps extends LayoutProps {
  /** Transcript props forwarded to {@link ChatMessageList}. */
  readonly list?: Omit<ChatMessageListProps, 'layout'>
  /** Input props forwarded to {@link ChatInputPanel}. */
  readonly input?: Omit<ChatInputPanelProps, 'layout'>
  /** Vertical spacing between the transcript and the input panel. */
  readonly spacing?: number
}

/**
 * Convenience composite: a {@link ChatMessageList} that fills the available
 * space stacked above a content-height {@link ChatInputPanel}. Mirrors the
 * Rust-side `ChatPanel` composition.
 */
export function ChatPanel({ list, input, spacing, layout }: ChatPanelProps): ReactElement {
  return hostElement(
    'vstack',
    { spacing, layout },
    createElement(ChatMessageList, { ...list, layout: { height: 'fill' } }),
    createElement(ChatInputPanel, { ...input, layout: { height: 'content' } }),
  )
}

/** Vertical stack wrapper with camelCase props matching the Rust component schema. */
export function VStack({ spacing, padding, scrollable, children, layout }: StackProps): ReactElement {
  return hostElement('vstack', { spacing, padding, scrollable, layout }, children)
}

/** Horizontal stack wrapper with the same props as `VStack`. */
export function HStack({ spacing, padding, scrollable, children, layout }: StackProps): ReactElement {
  return hostElement('hstack', { spacing, padding, scrollable, layout }, children)
}

/** Grid wrapper that maps camelCase gaps to runtime snake_case properties. */
export function Grid({ columns, rowGap, columnGap, padding, scrollable, children, layout }: GridProps): ReactElement {
  return hostElement('grid', {
    columns,
    row_gap: rowGap,
    column_gap: columnGap,
    padding,
    scrollable,
    layout,
  }, children)
}

function hostElement(type: string, props: Record<string, unknown>, ...children: ReactNode[]): ReactElement {
  return createElement(type, props, ...children)
}

function controlledTextChange(
  componentName: string,
  onChange: ValueChangeHandler<string>,
): AttoUiEventHandler {
  return (event) => onChange(stringPayload(componentName, event), event)
}

function selectionHandler(
  componentName: string,
  onChange: ValueChangeHandler<number> | undefined,
  onSelect: ValueChangeHandler<number> | undefined,
): AttoUiEventHandler | undefined {
  if (onChange === undefined && onSelect === undefined) return undefined
  return (event) => {
    const selected = numberPayload(componentName, event)
    onChange?.(selected, event)
    onSelect?.(selected, event)
  }
}

function fileTreeSelectHandler(onSelect: FileTreeSelectHandler | undefined): AttoUiEventHandler | undefined {
  if (onSelect === undefined) return undefined
  return (event) => {
    const payload = event.payload
    const nodeId = typeof payload === 'number' && Number.isFinite(payload) ? payload : null
    onSelect(nodeId, event)
  }
}

function fileTreeRenameHandler(onRename: FileTreeRenameHandler | undefined): AttoUiEventHandler | undefined {
  if (onRename === undefined) return undefined
  return (event) => {
    const map = mapPayload('FileTree', event)
    onRename(
      {
        id: numberField(map, 'id'),
        kind: stringField(map, 'kind'),
        oldName: stringField(map, 'old_name'),
        newName: stringField(map, 'new_name'),
      },
      event,
    )
  }
}

function fileTreeDeleteHandler(onDelete: FileTreeDeleteHandler | undefined): AttoUiEventHandler | undefined {
  if (onDelete === undefined) return undefined
  return (event) => {
    const map = mapPayload('FileTree', event)
    onDelete(
      {
        id: numberField(map, 'id'),
        kind: stringField(map, 'kind'),
        name: stringField(map, 'name'),
      },
      event,
    )
  }
}

function slashCommandHandler(onSlashCommand: ChatSlashCommandHandler | undefined): AttoUiEventHandler | undefined {
  if (onSlashCommand === undefined) return undefined
  return (event) => {
    const map = mapPayload('ChatInputPanel slash_command', event)
    onSlashCommand({
      id: optionalStringField(map, 'id'),
      label: stringField(map, 'label'),
      detail: optionalStringField(map, 'detail'),
      replacement: optionalStringField(map, 'replacement'),
      action: slashCommandActionField(map),
    }, event)
  }
}

function mentionQueryHandler(onMentionQuery: ChatMentionQueryHandler | undefined): AttoUiEventHandler | undefined {
  if (onMentionQuery === undefined) return undefined
  return (event) => {
    const map = mapPayload('ChatInputPanel mention_query', event)
    onMentionQuery({
      draft: stringField(map, 'draft'),
      query: stringField(map, 'query'),
      cursor: numberField(map, 'cursor'),
      replacement_start: numberField(map, 'replacement_start'),
      replacement_end: numberField(map, 'replacement_end'),
    }, event)
  }
}

function mapPayload(componentName: string, event: AttoUiCallbackEvent): Record<string, unknown> {
  const payload = event.payload
  if (payload !== null && typeof payload === 'object' && !Array.isArray(payload)) {
    return payload as Record<string, unknown>
  }
  throw new Error(`${componentName} event expected a map payload`)
}

function numberField(map: Record<string, unknown>, key: string): number {
  const value = map[key]
  return typeof value === 'number' && Number.isFinite(value) ? value : 0
}

function stringField(map: Record<string, unknown>, key: string): string {
  const value = map[key]
  return typeof value === 'string' ? value : ''
}

function optionalStringField(map: Record<string, unknown>, key: string): string | undefined {
  const value = map[key]
  return typeof value === 'string' ? value : undefined
}

function slashCommandActionField(map: Record<string, unknown>): ChatSlashCommandAction | undefined {
  const value = optionalStringField(map, 'action')
  return value === 'insert' || value === 'submit' ? value : undefined
}

function childrenLabel(children: PrimitiveLabel | undefined): string | undefined {
  return children === undefined ? undefined : String(children)
}

function stringPayload(componentName: string, event: AttoUiCallbackEvent): string {
  if (typeof event.payload === 'string') return event.payload
  throw new Error(`${componentName} change event expected a string payload`)
}

function numberPayload(componentName: string, event: AttoUiCallbackEvent): number {
  if (typeof event.payload === 'number' && Number.isFinite(event.payload)) return event.payload
  throw new Error(`${componentName} change event expected a numeric payload`)
}

function boolPayload(componentName: string, event: AttoUiCallbackEvent): boolean {
  if (typeof event.payload === 'boolean') return event.payload
  throw new Error(`${componentName} change event expected a boolean payload`)
}

export interface CheckboxProps extends LayoutProps {
  readonly label?: string
  readonly checked?: boolean
  readonly enabled?: boolean
  readonly onChange?: ValueChangeHandler<boolean>
}

/** Toggle checkbox; `onChange` receives the new checked state. */
export function Checkbox({ label, checked, enabled, onChange, layout }: CheckboxProps): ReactElement {
  return hostElement('checkbox', {
    label,
    checked,
    enabled,
    onChange: onChange === undefined ? undefined : (event: AttoUiCallbackEvent) => onChange(boolPayload('Checkbox', event), event),
    layout,
  })
}

export interface RadioGroupProps extends LayoutProps {
  readonly label?: string
  readonly options: readonly string[]
  readonly selectedIndex?: number
  readonly enabled?: boolean
  readonly height?: number
  readonly onChange?: ValueChangeHandler<number>
}

/** Radio button group; `onChange` receives the selected option index. */
export function RadioGroup(props: RadioGroupProps): ReactElement {
  const { label, options, selectedIndex, enabled, height, onChange, layout } = props
  return hostElement('radioGroup', {
    label,
    options,
    selection: selectedIndex ?? 0,
    enabled,
    height,
    onChange: onChange === undefined ? undefined : (event: AttoUiCallbackEvent) => onChange(numberPayload('RadioGroup', event), event),
    layout,
  })
}

export interface SliderProps extends LayoutProps {
  readonly min?: number
  readonly max?: number
  readonly value: number
  readonly step?: number
  readonly enabled?: boolean
  readonly onChange?: ValueChangeHandler<number>
}

/** Horizontal slider; `onChange` receives the new numeric value. */
export function Slider({ min, max, value, step, enabled, onChange, layout }: SliderProps): ReactElement {
  return hostElement('slider', {
    min,
    max,
    value,
    step,
    enabled,
    onChange: onChange === undefined ? undefined : (event: AttoUiCallbackEvent) => onChange(numberPayload('Slider', event), event),
    layout,
  })
}

export interface ProgressBarProps extends LayoutProps {
  readonly min?: number
  readonly max?: number
  readonly value: number
  readonly showText?: boolean
  readonly text?: string
  readonly enabled?: boolean
}

/** Determinate progress bar. */
export function ProgressBar({ min, max, value, showText, text, enabled, layout }: ProgressBarProps): ReactElement {
  return hostElement('progressBar', { min, max, value, show_text: showText, text, enabled, layout })
}

export interface SpinnerProps extends LayoutProps {
  readonly text?: string
  readonly running?: boolean
  readonly enabled?: boolean
}

/** Indeterminate spinner with optional label. */
export function Spinner({ text, running, enabled, layout }: SpinnerProps): ReactElement {
  return hostElement('spinner', { text, running, enabled, layout })
}

/** Static text label. */
export function Label({ text, enabled, layout }: LabelProps): ReactElement {
  return hostElement('label', { text, enabled, layout })
}

export interface DividerProps extends LayoutProps {
  readonly orientation?: 'horizontal' | 'vertical'
}

/** Horizontal or vertical separator line. */
export function Divider({ orientation, layout }: DividerProps = {}): ReactElement {
  return hostElement('divider', { orientation, layout })
}

export interface BorderProps extends LayoutProps {
  readonly border?: boolean
  readonly children?: ReactNode
}

/** Draws a border around a single child. */
export function Border({ border, children, layout }: BorderProps): ReactElement {
  return hostElement('border', { border, layout }, children)
}

export interface DisclosureProps extends LayoutProps {
  readonly title: string
  readonly expanded?: boolean
  readonly status?: string
  readonly content?: string
  readonly enabled?: boolean
  readonly onToggle?: ValueChangeHandler<boolean>
  readonly children?: ReactNode
}

/** Collapsible section; `onToggle` receives the new expanded state. */
export function Disclosure(props: DisclosureProps): ReactElement {
  const { title, expanded, status, content, enabled, onToggle, children, layout } = props
  return hostElement('disclosure', {
    title,
    expanded,
    status,
    content,
    enabled,
    onToggle: onToggle === undefined ? undefined : (event: AttoUiCallbackEvent) => onToggle(boolPayload('Disclosure', event), event),
    layout,
  }, children)
}

export interface TextAreaProps extends LayoutProps {
  readonly title?: string
  readonly value: string
  readonly placeholder?: string
  readonly enabled?: boolean
  readonly clipboard?: string
  readonly height?: number
  readonly enterSubmits?: boolean
  readonly onChange?: ValueChangeHandler<string>
  readonly onSubmit?: AttoUiEventHandler
}

/** Controlled multi-line text area; pass `value` and update it from `onChange`. */
export function TextArea(props: TextAreaProps): ReactElement {
  const { title, value, placeholder, enabled, clipboard, height, enterSubmits, onChange, onSubmit, layout } = props
  return hostElement('textArea', {
    __attoControlledText: true,
    title,
    text: value,
    placeholder,
    enabled,
    clipboard,
    height,
    enter_submits: enterSubmits,
    onChange: onChange === undefined ? undefined : controlledTextChange('TextArea', onChange),
    onSubmit,
    layout,
  })
}

export interface EditorProps extends LayoutProps {
  readonly value?: string
  readonly languageId?: string
  readonly showLineNumbers?: boolean
  readonly showFoldingMarkers?: boolean
  readonly readOnly?: boolean
  readonly tabWidth?: number
  readonly insertSpaces?: boolean
}

/** Code editor view (syntax via `languageId`). */
export function Editor(props: EditorProps): ReactElement {
  const { value, languageId, showLineNumbers, showFoldingMarkers, readOnly, tabWidth, insertSpaces, layout } = props
  return hostElement('editor', {
    text: value,
    language_id: languageId,
    show_line_numbers: showLineNumbers,
    show_folding_markers: showFoldingMarkers,
    read_only: readOnly,
    tab_width: tabWidth,
    insert_spaces: insertSpaces,
    layout,
  })
}
