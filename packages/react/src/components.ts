import { createElement, type ReactElement, type ReactNode } from 'react'
import type { EdgeInsetsSpec } from '@atto-ui/core'

import type { AttoUiCallbackEvent, AttoUiEventHandler } from './events'

export type ValueChangeHandler<T> = (value: T, event: AttoUiCallbackEvent) => void

export type PrimitiveLabel = string | number

export interface LabelProps {
  readonly text: string
  readonly enabled?: boolean
}

export interface ButtonHostProps {
  readonly label?: string
  readonly enabled?: boolean
  readonly onClick?: AttoUiEventHandler
}

export interface ButtonProps extends ButtonHostProps {
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

export interface TextBoxProps extends Omit<TextBoxHostProps, 'text' | 'onChange'> {
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

export interface ListBoxProps extends Omit<ListBoxHostProps, 'onChange'> {
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

export interface TableProps extends Omit<TableViewHostProps, 'onChange'> {
  readonly rows: readonly (readonly string[])[]
  readonly selectedIndex?: number
  readonly onChange?: ValueChangeHandler<number>
  readonly onSelect?: ValueChangeHandler<number>
}

export interface StackProps {
  readonly spacing?: number
  readonly padding?: EdgeInsetsSpec
  readonly scrollable?: boolean
  readonly children?: ReactNode
}

export interface GridProps {
  readonly columns?: number
  readonly rowGap?: number
  readonly columnGap?: number
  readonly padding?: EdgeInsetsSpec
  readonly scrollable?: boolean
  readonly children?: ReactNode
}

/** Typed React wrapper for the native Button host component. */
export function Button({ label, children, enabled, onClick }: ButtonProps): ReactElement {
  return hostElement('button', {
    label: label ?? childrenLabel(children) ?? 'Button',
    enabled,
    onClick,
  })
}

/** Controlled single-line TextBox wrapper using the runtime `text` property. */
export function TextBox(props: TextBoxProps): ReactElement {
  const { title, value, placeholder, enabled, clipboard, onChange, onSubmit } = props
  return hostElement('textBox', {
    __attoControlledText: true,
    title,
    text: value,
    placeholder,
    enabled,
    clipboard,
    onChange: onChange === undefined ? undefined : controlledTextChange(onChange),
    onSubmit,
  })
}

/** Typed ListBox wrapper; `onSelect` and `onChange` receive the selected index. */
export function ListBox(props: ListBoxProps): ReactElement {
  const { title, items, selection, selectedIndex, height, enabled, onChange, onSelect } = props
  return hostElement('listBox', {
    title,
    items,
    selection: selectedIndex ?? selection ?? 0,
    height,
    enabled,
    onChange: selectionHandler('ListBox', onChange, onSelect),
  })
}

/** Typed TableView wrapper with the shorter exported name `Table`. */
export function Table(props: TableProps): ReactElement {
  const { title, headers, rows, selection, selectedIndex, height, enabled, onChange, onSelect } = props
  return hostElement('tableView', {
    title,
    headers,
    rows,
    selection: selectedIndex ?? selection ?? 0,
    height,
    enabled,
    onChange: selectionHandler('Table', onChange, onSelect),
  })
}

export const TableView = Table

/** Vertical stack wrapper with camelCase props matching the Rust component schema. */
export function VStack({ spacing, padding, scrollable, children }: StackProps): ReactElement {
  return hostElement('vstack', { spacing, padding, scrollable }, children)
}

/** Horizontal stack wrapper with the same props as `VStack`. */
export function HStack({ spacing, padding, scrollable, children }: StackProps): ReactElement {
  return hostElement('hstack', { spacing, padding, scrollable }, children)
}

/** Grid wrapper that maps camelCase gaps to runtime snake_case properties. */
export function Grid({ columns, rowGap, columnGap, padding, scrollable, children }: GridProps): ReactElement {
  return hostElement('grid', {
    columns,
    row_gap: rowGap,
    column_gap: columnGap,
    padding,
    scrollable,
  }, children)
}

function hostElement(type: string, props: Record<string, unknown>, ...children: ReactNode[]): ReactElement {
  return createElement(type, props, ...children)
}

function controlledTextChange(onChange: ValueChangeHandler<string>): AttoUiEventHandler {
  return (event) => onChange(stringPayload('TextBox', event), event)
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
