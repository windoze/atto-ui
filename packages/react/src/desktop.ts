import { Fragment, createElement, type ReactElement, type ReactNode } from 'react'
import type { RectLike } from '@atto-ui/core'

export interface WindowProps {
  readonly title?: string
  readonly rect: RectLike
  readonly children?: ReactNode
}

export interface DesktopProps {
  readonly children?: ReactNode
}

export interface StatusBarProps {
  readonly left?: string | null
  readonly right?: string | null
}

export interface MenuBarProps {
  readonly children?: ReactNode
}

export interface MenuProps {
  readonly id?: string
  readonly title: string
  readonly children?: ReactNode
}

export interface MenuItemProps {
  readonly id?: string
  readonly label: string
  readonly shortcut?: string | null
  readonly enabled?: boolean
  readonly onClick?: () => void
  readonly children?: ReactNode
}

/** Optional readability wrapper; the reconciler itself supplies the virtual DesktopContainer. */
export function Desktop({ children }: DesktopProps): ReactElement {
  return createElement(Fragment, null, children)
}

/** Virtual desktop child that owns one native atto-ui window. */
export function Window({ title, rect, children }: WindowProps): ReactElement {
  return createElement('window', { title, rect }, children)
}

/** Virtual desktop child that replaces the native menu bar slot. */
export function MenuBar({ children }: MenuBarProps): ReactElement {
  return createElement('menuBar', null, children)
}

/** Menu spec node used under `MenuBar`. */
export function Menu({ id, title, children }: MenuProps): ReactElement {
  return createElement('menu', { id, title }, children)
}

/** Menu item spec node used under `Menu` or another `MenuItem`. */
export function MenuItem(props: MenuItemProps): ReactElement {
  const { id, label, shortcut, enabled, onClick, children } = props
  return createElement('menuItem', { id, label, shortcut, enabled, onClick }, children)
}

/** Virtual desktop child that replaces the native status bar text slot. */
export function StatusBar({ left, right }: StatusBarProps): ReactElement {
  return createElement('statusBar', { left, right })
}
