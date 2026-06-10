import { Fragment, createElement, type ReactElement, type ReactNode } from 'react'
import type { RectLike } from '@atto-ui/core'

import type { AttoUiEventHandler, AttoUiWindowEventHandler } from './events'

export type OneOrMany<T> = T | readonly T[]
export type MenuItemElement = ReactElement<MenuItemProps, typeof MenuItem>
export type MenuElement = ReactElement<MenuProps, typeof Menu>
export type MenuBarElement = ReactElement<MenuBarProps, typeof MenuBar>
export type StatusBarElement = ReactElement<StatusBarProps, typeof StatusBar>
export type WindowElement = ReactElement<WindowProps, typeof Window>
export type MenuItemChildren = OneOrMany<MenuItemElement | null | false>
export type MenuBarChildren = OneOrMany<MenuElement | null | false>
export type DesktopChildren = OneOrMany<WindowElement | MenuBarElement | StatusBarElement | null | false>

export interface WindowProps {
  readonly title?: string
  readonly rect: RectLike
  readonly children?: ReactNode
  /** Fired when the user closes the window from the TUI (titlebar button / menu). */
  readonly onClose?: AttoUiWindowEventHandler
  /** Fired when the user minimizes the window from the TUI. */
  readonly onMinimize?: AttoUiWindowEventHandler
  /** Fired when the user maximizes the window from the TUI. */
  readonly onMaximize?: AttoUiWindowEventHandler
  /** Fired when a minimized/maximized window returns to normal from the TUI. */
  readonly onRestore?: AttoUiWindowEventHandler
}

export interface DesktopProps {
  readonly children?: DesktopChildren
}

export interface StatusBarProps {
  readonly left?: string | null
  readonly right?: string | null
  readonly children?: never
}

export interface MenuBarProps {
  readonly children?: MenuBarChildren
}

export interface MenuProps {
  readonly id?: string
  readonly title: string
  readonly children?: MenuItemChildren
}

export interface MenuItemProps {
  readonly id?: string
  readonly label: string
  readonly shortcut?: string | null
  readonly enabled?: boolean
  readonly onClick?: AttoUiEventHandler
  readonly children?: MenuItemChildren
}

/** Optional readability wrapper; the reconciler itself supplies the virtual DesktopContainer. */
export function Desktop({ children }: DesktopProps): ReactElement {
  return createElement(Fragment, null, children)
}

/** Virtual desktop child that owns one native atto-ui window. */
export function Window(props: WindowProps): ReactElement {
  const { title, rect, children, onClose, onMinimize, onMaximize, onRestore } = props
  return createElement('window', { title, rect, onClose, onMinimize, onMaximize, onRestore }, children)
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
