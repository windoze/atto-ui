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

/**
 * Reserved menu item id recognized by the native runtime. A `<MenuItem>` (or
 * `<MinimizedWindowsMenu>`) carrying this id is auto-populated each frame with
 * the currently minimized windows; selecting an entry restores that window.
 * The submenu is owned by Rust — do not give such an item your own children.
 */
export const MINIMIZED_WINDOWS_MENU_ID = 'atto_ui:minimized_windows'

export interface MinimizedWindowsMenuProps {
  /** Label shown in the parent menu. Defaults to `"Minimized windows"`. */
  readonly label?: string
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

/**
 * Menu item that the native runtime fills with the list of minimized windows.
 * Place it under a `<Menu>` (typically a "Window" menu). The runtime refreshes
 * the submenu every frame and restores the chosen window on click, so there is
 * no `onClick` to wire up and no children to provide.
 */
export function MinimizedWindowsMenu({ label = 'Minimized windows' }: MinimizedWindowsMenuProps): ReactElement {
  return createElement('menuItem', { id: MINIMIZED_WINDOWS_MENU_ID, label })
}

/**
 * Standard window operations recognized by the native runtime. A `<MenuItem>`
 * (or `<WindowOpMenuItem>`) carrying the matching id performs the operation
 * natively when selected — no `onClick` wiring is required, and any callback is
 * ignored. To customize behavior, use a plain `<MenuItem>` with your own id.
 */
export type WindowMenuOp =
  | 'cascade'
  | 'tile'
  | 'minimize'
  | 'maximize'
  | 'restore'
  | 'close'
  | 'next'
  | 'previous'
  | 'minimizeAll'
  | 'restoreAll'
  | 'closeAll'

/** Reserved menu item ids, keyed by operation. Mirrors the Rust `atto_ui:window_*` ids. */
export const WINDOW_OP_MENU_IDS: Record<WindowMenuOp, string> = {
  cascade: 'atto_ui:window_cascade',
  tile: 'atto_ui:window_tile',
  minimize: 'atto_ui:window_minimize',
  maximize: 'atto_ui:window_maximize',
  restore: 'atto_ui:window_restore',
  close: 'atto_ui:window_close',
  next: 'atto_ui:window_next',
  previous: 'atto_ui:window_previous',
  minimizeAll: 'atto_ui:window_minimize_all',
  restoreAll: 'atto_ui:window_restore_all',
  closeAll: 'atto_ui:window_close_all',
}

const WINDOW_OP_DEFAULT_LABELS: Record<WindowMenuOp, string> = {
  cascade: 'Cascade',
  tile: 'Tile',
  minimize: 'Minimize',
  maximize: 'Maximize',
  restore: 'Restore',
  close: 'Close',
  next: 'Next Window',
  previous: 'Previous Window',
  minimizeAll: 'Minimize All',
  restoreAll: 'Restore All',
  closeAll: 'Close All',
}

export interface WindowOpMenuItemProps {
  /** Which standard window operation this item performs. */
  readonly op: WindowMenuOp
  /** Optional label override. Defaults to a sensible name for the operation. */
  readonly label?: string
  /** Optional shortcut hint shown in the menu. */
  readonly shortcut?: string | null
  /** Whether the item is selectable. Defaults to `true`. */
  readonly enabled?: boolean
}

/**
 * Menu item bound to a standard window operation (cascade, tile, minimize,
 * maximize, …). The native runtime runs the operation on selection; the
 * `maximize`/`minimize` ops act on the focused window. Place under a `<Menu>`.
 */
export function WindowOpMenuItem({ op, label, shortcut, enabled }: WindowOpMenuItemProps): ReactElement {
  return createElement('menuItem', {
    id: WINDOW_OP_MENU_IDS[op],
    label: label ?? WINDOW_OP_DEFAULT_LABELS[op],
    shortcut,
    enabled,
  })
}

/** Virtual desktop child that replaces the native status bar text slot. */
export function StatusBar({ left, right }: StatusBarProps): ReactElement {
  return createElement('statusBar', { left, right })
}
