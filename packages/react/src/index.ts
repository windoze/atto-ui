export {
  createDesktopRoot,
  createRoot,
  renderToWindow,
  type AttoRoot,
  type AttoRootOptions,
} from './reconciler'

export {
  render,
  type RenderHandle,
  type RenderOptions,
} from './render'

export {
  createHostContainer,
  createDesktopHostContainer,
  dispatchHostCallbacks,
  flushStaticTree,
  normalizeHostType,
  sanitizeProps,
  toComponentSpec,
  type HostContainer,
  type HostContainerOptions,
  type HostInstance,
  type HostProps,
  type DesktopRenderHost,
  type RenderHost,
} from './host'

export {
  CallbackEventDispatcher,
  type AttoUiCallbackEvent,
  type AttoUiEventHandler,
} from './events'

export {
  Desktop,
  Menu,
  MenuBar,
  MenuItem,
  StatusBar,
  Window,
  type DesktopProps,
  type MenuBarProps,
  type MenuItemProps,
  type MenuProps,
  type StatusBarProps,
  type WindowProps,
} from './desktop'

export {
  Button,
  Grid,
  HStack,
  ListBox,
  Table,
  TableView,
  TextBox,
  VStack,
  type ButtonHostProps,
  type ButtonProps,
  type GridProps,
  type LabelProps,
  type ListBoxHostProps,
  type ListBoxProps,
  type StackProps,
  type TableProps,
  type TableViewHostProps,
  type TextBoxHostProps,
  type TextBoxProps,
  type ValueChangeHandler,
} from './components'

export {
  B,
  I,
  Link,
  Markdown,
  S,
  Text,
  U,
  type LinkClickHandler,
  type LinkProps,
  type MarkdownProps,
  type TextProps,
} from './text'

export type { RichTextHostProps } from './jsx'
