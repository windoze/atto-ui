import type { ReactNode } from 'react'

import type {
  ButtonHostProps,
  ChatMessageListHostProps,
  FileTreeHostProps,
  GridProps,
  LabelProps,
  ListBoxHostProps,
  StackProps,
  TableViewHostProps,
  TextBoxHostProps,
} from './components'
import type { AttoUiEventHandler } from './events'
import type {
  MenuBarProps,
  MenuItemProps,
  MenuProps,
  StatusBarProps,
  WindowProps,
} from './desktop'
import type { MarkdownProps } from './text'

interface TextSpanHostProps {
  readonly text?: string
  readonly bold?: boolean
  readonly italic?: boolean
  readonly underline?: boolean
  readonly strike?: boolean
  readonly color?: string
  readonly href?: string
}

interface RichTextHostProps {
  readonly onLink?: AttoUiEventHandler
  readonly children?: ReactNode
}

interface MarkdownViewerHostProps {
  readonly markdown?: string
  readonly wrap_width?: number
  readonly show_markers?: boolean
  readonly vertical_scrollbar?: 'always' | 'auto' | 'never'
  readonly code_block_max_height?: number
  readonly table_max_height?: number
  readonly onLink?: AttoUiEventHandler
}

interface GridHostProps {
  readonly columns?: number
  readonly row_gap?: number
  readonly column_gap?: number
  readonly padding?: GridProps['padding']
  readonly scrollable?: boolean
  readonly children?: ReactNode
}

interface ProgressBarHostProps {
  readonly min?: number
  readonly max?: number
  readonly value?: number
  readonly enabled?: boolean
  readonly show_text?: boolean
  readonly text?: string
}

interface RadioGroupHostProps {
  readonly label?: string
  readonly options?: readonly string[]
  readonly selection?: number
  readonly enabled?: boolean
  readonly height?: number
  readonly onChange?: AttoUiEventHandler
}

interface SliderHostProps {
  readonly min?: number
  readonly max?: number
  readonly value?: number
  readonly step?: number
  readonly enabled?: boolean
  readonly onChange?: AttoUiEventHandler
}

declare global {
  namespace JSX {
    interface IntrinsicElements {
      checkbox: {
        readonly label?: string
        readonly checked?: boolean
        readonly enabled?: boolean
        readonly onChange?: AttoUiEventHandler
      }
      chatMessageList: ChatMessageListHostProps
      chatmessagelist: ChatMessageListHostProps
      fileTree: FileTreeHostProps
      filetree: FileTreeHostProps
      grid: GridHostProps
      hstack: StackProps
      listBox: ListBoxHostProps
      listbox: ListBoxHostProps
      markdownViewer: MarkdownViewerHostProps
      markdownviewer: MarkdownViewerHostProps
      menuBar: MenuBarProps
      menubar: MenuBarProps
      menuItem: MenuItemProps
      progressBar: ProgressBarHostProps
      progressbar: ProgressBarHostProps
      radioGroup: RadioGroupHostProps
      radiogroup: RadioGroupHostProps
      richText: RichTextHostProps
      richtext: RichTextHostProps
      slider: SliderHostProps
      spacer: Record<string, never>
      spinner: {
        readonly text?: string
        readonly enabled?: boolean
        readonly running?: boolean
      }
      statusBar: StatusBarProps
      statusbar: StatusBarProps
      tableView: TableViewHostProps
      tableview: TableViewHostProps
      textBox: TextBoxHostProps
      textbox: TextBoxHostProps
      textSpan: TextSpanHostProps
      textspan: TextSpanHostProps
      vstack: StackProps
      window: WindowProps
    }
  }
}

export type {
  ButtonHostProps,
  ChatMessageListHostProps,
  FileTreeHostProps,
  LabelProps,
  MarkdownProps,
  GridHostProps,
  RichTextHostProps,
  TableViewHostProps,
  TextBoxHostProps,
}
