import { createElement, type ReactElement, type ReactNode } from 'react'
import type { EdgeInsetsSpec, LayoutSpec } from '@atto-ui/core'

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
  const { title, value, placeholder, enabled, clipboard, onChange, onSubmit, layout } = props
  return hostElement('textBox', {
    __attoControlledText: true,
    title,
    text: value,
    placeholder,
    enabled,
    clipboard,
    onChange: onChange === undefined ? undefined : controlledTextChange('TextBox', onChange),
    onSubmit,
    layout,
  })
}

/** Typed ListBox wrapper; `onSelect` and `onChange` receive the selected index. */
export function ListBox(props: ListBoxProps): ReactElement {
  const { title, items, selection, selectedIndex, height, enabled, onChange, onSelect, layout } = props
  return hostElement('listBox', {
    title,
    items,
    selection: selectedIndex ?? selection ?? 0,
    height,
    enabled,
    onChange: selectionHandler('ListBox', onChange, onSelect),
    layout,
  })
}

/** Typed TableView wrapper with the shorter exported name `Table`. */
export function Table(props: TableProps): ReactElement {
  const { title, headers, rows, selection, selectedIndex, height, enabled, onChange, onSelect, layout } = props
  return hostElement('tableView', {
    title,
    headers,
    rows,
    selection: selectedIndex ?? selection ?? 0,
    height,
    enabled,
    onChange: selectionHandler('Table', onChange, onSelect),
    layout,
  })
}

export const TableView = Table

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
