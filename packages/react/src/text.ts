import {
  Children,
  Fragment,
  createElement,
  isValidElement,
  type ReactElement,
  type ReactNode,
} from 'react'

import type { LayoutProps } from './components'
import type { AttoUiCallbackEvent, AttoUiEventHandler } from './events'

type InlineKind = 'bold' | 'italic' | 'underline' | 'strike' | 'link'

type InlineComponent<P> = ((props: P) => ReactElement | null) & {
  __attoTextInlineKind?: InlineKind
}

interface InlineStyle {
  readonly bold: boolean
  readonly italic: boolean
  readonly underline: boolean
  readonly strike: boolean
  readonly href: string | null
}

interface SpanDescriptor {
  readonly text: string
  readonly style: InlineStyle
}

interface LinkRoute {
  readonly href: string
  readonly onClick: LinkClickHandler
}

interface InlineProps {
  readonly children?: ReactNode
}

export type LinkClickHandler = (event: AttoUiCallbackEvent) => void

export interface TextProps extends LayoutProps {
  readonly children?: ReactNode
  readonly onLink?: AttoUiEventHandler
  readonly onLinkClick?: AttoUiEventHandler
}

export interface LinkProps extends InlineProps {
  readonly href: string
  readonly onClick?: LinkClickHandler
}

export interface MarkdownProps extends LayoutProps {
  readonly children?: ReactNode
  readonly markdown?: string
  readonly onLink?: AttoUiEventHandler
  readonly wrapWidth?: number
  readonly showMarkers?: boolean
  readonly verticalScrollbar?: 'always' | 'auto' | 'never'
  readonly codeBlockMaxHeight?: number
  readonly tableMaxHeight?: number
}

const EMPTY_STYLE: InlineStyle = Object.freeze({
  bold: false,
  italic: false,
  underline: false,
  strike: false,
  href: null,
})

const BoldMarker = inlineMarker('bold')
const ItalicMarker = inlineMarker('italic')
const UnderlineMarker = inlineMarker('underline')
const StrikeMarker = inlineMarker('strike')
const LinkMarker = inlineMarker<LinkProps>('link')

/** Render inline React text as a RichText container backed by TextSpan children. */
export function Text(props: TextProps): ReactElement {
  const links: LinkRoute[] = []
  const spans = flattenInlineChildren(props.children, EMPTY_STYLE, links)
  const richTextProps: Record<string, unknown> = {}
  const onLink = linkHandler(links, props.onLink, props.onLinkClick)
  if (onLink) {
    richTextProps.onLink = onLink
  }
  if (props.layout) {
    richTextProps.layout = props.layout
  }

  return createElement(
    'richText',
    richTextProps,
    spans.map((span, index) => createTextSpan(span, index)),
  )
}

export const B = inlineComponent<InlineProps>('bold', BoldMarker)
export const I = inlineComponent<InlineProps>('italic', ItalicMarker)
export const U = inlineComponent<InlineProps>('underline', UnderlineMarker)
export const S = inlineComponent<InlineProps>('strike', StrikeMarker)
export const Link = inlineComponent<LinkProps>('link', LinkMarker)

/** Render block markdown through the native MarkdownViewer component. */
export function Markdown(props: MarkdownProps): ReactElement {
  const {
    children,
    markdown,
    wrapWidth,
    showMarkers,
    verticalScrollbar,
    codeBlockMaxHeight,
    tableMaxHeight,
    onLink,
    layout,
  } = props
  const viewerProps: Record<string, unknown> = {
    markdown: markdown ?? textContent(children),
  }

  if (wrapWidth !== undefined) viewerProps.wrap_width = wrapWidth
  if (showMarkers !== undefined) viewerProps.show_markers = showMarkers
  if (verticalScrollbar !== undefined) viewerProps.vertical_scrollbar = verticalScrollbar
  if (codeBlockMaxHeight !== undefined) viewerProps.code_block_max_height = codeBlockMaxHeight
  if (tableMaxHeight !== undefined) viewerProps.table_max_height = tableMaxHeight
  if (onLink !== undefined) viewerProps.onLink = onLink
  if (layout !== undefined) viewerProps.layout = layout

  return createElement('markdownViewer', viewerProps)
}

function inlineMarker<P extends InlineProps = InlineProps>(kind: InlineKind): InlineComponent<P> {
  const marker = (() => null) as InlineComponent<P>
  marker.__attoTextInlineKind = kind
  return marker
}

function inlineComponent<P extends InlineProps>(
  kind: InlineKind,
  marker: InlineComponent<P>,
): InlineComponent<P> {
  const component = ((props: P) => (
    createElement(Text, null, createElement(marker, props))
  )) as InlineComponent<P>
  component.__attoTextInlineKind = kind
  return component
}

function flattenInlineChildren(
  children: ReactNode,
  style: InlineStyle,
  links: LinkRoute[],
): SpanDescriptor[] {
  const spans: SpanDescriptor[] = []
  Children.forEach(children, (child) => {
    appendInlineChild(child, style, links, spans)
  })
  return spans
}

function appendInlineChild(
  child: ReactNode,
  style: InlineStyle,
  links: LinkRoute[],
  spans: SpanDescriptor[],
): void {
  if (child === null || child === undefined || typeof child === 'boolean') return
  if (typeof child === 'string' || typeof child === 'number') {
    const text = String(child)
    if (text.length > 0) {
      spans.push({ text, style })
    }
    return
  }

  if (!isValidElement(child)) {
    throw new Error('Text accepts only text, fragments, and inline text components')
  }

  const element = child as ReactElement<Record<string, unknown>>
  if (element.type === Fragment) {
    spans.push(...flattenInlineChildren(element.props.children as ReactNode, style, links))
    return
  }

  const kind = inlineKind(element.type)
  if (kind === null) {
    throw new Error('Text accepts only text, fragments, and inline text components')
  }

  const nextStyle = styleForInline(kind, style, element.props, links)
  spans.push(...flattenInlineChildren(element.props.children as ReactNode, nextStyle, links))
}

function inlineKind(type: unknown): InlineKind | null {
  if (typeof type !== 'function') return null
  return (type as InlineComponent<InlineProps>).__attoTextInlineKind ?? null
}

function styleForInline(
  kind: InlineKind,
  style: InlineStyle,
  props: Readonly<Record<string, unknown>>,
  links: LinkRoute[],
): InlineStyle {
  switch (kind) {
    case 'bold':
      return { ...style, bold: true }
    case 'italic':
      return { ...style, italic: true }
    case 'underline':
      return { ...style, underline: true }
    case 'strike':
      return { ...style, strike: true }
    case 'link': {
      const href = props.href
      if (typeof href !== 'string') return style
      const onClick = props.onClick
      if (typeof onClick === 'function') {
        links.push({ href, onClick: onClick as LinkClickHandler })
      }
      return { ...style, href }
    }
  }
}

function createTextSpan(span: SpanDescriptor, index: number): ReactElement {
  const props: Record<string, unknown> = {
    key: `span-${index}`,
    text: span.text,
  }
  if (span.style.bold) props.bold = true
  if (span.style.italic) props.italic = true
  if (span.style.underline) props.underline = true
  if (span.style.strike) props.strike = true
  if (span.style.href !== null) props.href = span.style.href
  return createElement('textSpan', props)
}

function linkHandler(
  links: readonly LinkRoute[],
  onLink: AttoUiEventHandler | undefined,
  onLinkClick: AttoUiEventHandler | undefined,
): AttoUiEventHandler | undefined {
  if (links.length === 0 && onLink === undefined && onLinkClick === undefined) {
    return undefined
  }

  return (event) => {
    const href = typeof event.payload === 'string' ? event.payload : null
    if (href !== null) {
      const route = links.find((candidate) => candidate.href === href)
      route?.onClick(event)
    }
    onLink?.(event)
    onLinkClick?.(event)
  }
}

function textContent(children: ReactNode): string {
  let out = ''
  Children.forEach(children, (child) => {
    if (child === null || child === undefined || typeof child === 'boolean') return
    if (typeof child === 'string' || typeof child === 'number') {
      out += String(child)
      return
    }
    if (isValidElement(child) && child.type === Fragment) {
      out += textContent((child as ReactElement<Record<string, unknown>>).props.children as ReactNode)
      return
    }
    throw new Error('Markdown accepts only text and fragments as children')
  })
  return out
}
