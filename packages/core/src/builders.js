'use strict'

function component(type, options = {}) {
  return makeSpec(type, options.id, options.props, options.events, options.children)
}

function child(node, options = {}) {
  if (options.layout === undefined && isEmptyRecord(options.meta)) return node
  const out = { node }
  if (options.layout !== undefined) out.layout = options.layout
  if (!isEmptyRecord(options.meta)) out.meta = options.meta
  return out
}

function withLayout(node, layout, meta) {
  return child(node, { layout, meta })
}

function withMeta(node, meta) {
  return child(node, { meta })
}

function tab(title, node, layout) {
  return child(node, { layout, meta: { title } })
}

function Text(text, options = {}) {
  return makeSpec('Text', options.id, {
    text,
    selectable: options.selectable,
    clipboard: options.clipboard,
  }, options.events)
}

function Label(text, options = {}) {
  return makeSpec('Label', options.id, {
    text,
    enabled: enabledValue(options),
  }, options.events)
}

function Button(first = {}, second = {}) {
  const options = typeof first === 'string' ? { ...second, label: first } : first
  return makeSpec('Button', options.id, {
    label: options.label ?? options.text,
    enabled: enabledValue(options),
  }, events(options.events, { click: options.onClick }))
}

function TextBox(options = {}) {
  return makeSpec('TextBox', options.id, {
    title: options.title,
    text: options.text,
    placeholder: options.placeholder,
    clipboard: options.clipboard,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange, submit: options.onSubmit }))
}

function TextArea(options = {}) {
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

function Checkbox(options = {}) {
  return makeSpec('Checkbox', options.id, {
    label: options.label,
    checked: options.checked,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange }))
}

function RadioGroup(options = {}) {
  return makeSpec('RadioGroup', options.id, {
    label: options.label,
    options: options.options,
    selection: options.selection,
    height: options.height,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange }))
}

function Slider(options = {}) {
  return makeSpec('Slider', options.id, {
    min: options.min,
    max: options.max,
    value: options.value,
    step: options.step,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange }))
}

function Spinner(textOrOptions = {}, options = {}) {
  const props = typeof textOrOptions === 'string' ? { ...options, text: textOrOptions } : textOrOptions
  return makeSpec('Spinner', props.id, {
    text: props.text,
    running: props.running,
    enabled: enabledValue(props),
  }, props.events)
}

function ProgressBar(options = {}) {
  return makeSpec('ProgressBar', options.id, {
    min: options.min,
    max: options.max,
    value: options.value,
    show_text: options.showText,
    text: options.text,
    enabled: enabledValue(options),
  }, options.events)
}

function ListBox(options = {}) {
  return makeSpec('ListBox', options.id, {
    title: options.title,
    items: options.items,
    selection: options.selection,
    height: options.height,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange }))
}

function TableView(options = {}) {
  return makeSpec('TableView', options.id, {
    title: options.title,
    headers: options.headers,
    rows: options.rows,
    selection: options.selection,
    height: options.height,
    enabled: enabledValue(options),
  }, events(options.events, { change: options.onChange }))
}

function VStack(first = {}, second) {
  const args = containerArgs(first, second)
  return makeSpec('VStack', args.options.id, stackProps(args.options), args.options.events, args.children)
}

function HStack(first = {}, second) {
  const args = containerArgs(first, second)
  return makeSpec('HStack', args.options.id, stackProps(args.options), args.options.events, args.children)
}

function Grid(first = {}, second) {
  const args = containerArgs(first, second)
  const options = args.options
  return makeSpec('Grid', options.id, {
    columns: options.columns,
    row_gap: options.rowGap,
    column_gap: options.columnGap,
    padding: options.padding,
    scrollable: options.scrollable,
  }, options.events, args.children)
}

function Border(node, options = {}) {
  return makeSpec('Border', options.id, { border: options.border }, options.events, [node])
}

function Visibility(node, options = {}) {
  return makeSpec('Visibility', options.id, { visible: options.visible }, options.events, [node])
}

function Divider(orientationOrOptions = {}, options = {}) {
  const props = typeof orientationOrOptions === 'string' ? { ...options, orientation: orientationOrOptions } : orientationOrOptions
  return makeSpec('Divider', props.id, { orientation: props.orientation }, props.events)
}

function Spacer(options = {}) {
  return makeSpec('Spacer', options.id, undefined, options.events)
}

function Splitter(first, second, options = {}) {
  return makeSpec('Splitter', options.id, {
    orientation: options.orientation,
    split_pos: options.splitPos,
    min_first: options.minFirst,
    min_second: options.minSecond,
    border: options.border,
  }, options.events, [first, second])
}

function TabView(first = {}, second) {
  const args = containerArgs(first, second)
  const options = args.options
  return makeSpec('TabView', options.id, {
    selection: options.selection,
    header_position: options.headerPosition,
  }, events(options.events, { change: options.onChange }), args.children)
}

function TextSpan(text, options = {}) {
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

function RichText(first = {}, second) {
  const args = containerArgs(first, second)
  const options = args.options
  return makeSpec('RichText', options.id, undefined, events(options.events, { link: options.onLink }), args.children)
}

function StyledLabel(text, options = {}) {
  return makeSpec('StyledLabel', options.id, {
    text,
    enabled: enabledValue(options),
  }, events(options.events, { link: options.onLink }))
}

function Disclosure(options = {}, children) {
  return makeSpec('Disclosure', options.id, {
    title: options.title,
    content: options.content,
    expanded: options.expanded,
    status: options.status,
    enabled: enabledValue(options),
  }, events(options.events, { toggle: options.onToggle }), children ?? options.children)
}

function TypeAhead(options = {}) {
  return makeSpec('TypeAhead', options.id, typeAheadProps(options), typeAheadEvents(options))
}

function CommandPalette(options = {}) {
  return makeSpec('CommandPalette', options.id, typeAheadProps(options), typeAheadEvents(options))
}

function makeSpec(type, id, props, eventInput, children) {
  const spec = { type }
  const compactProps = compactRecord(props)
  const compactEvents = compactEventRecord(eventInput)
  if (id !== undefined) spec.id = id
  if (compactProps !== undefined) spec.props = compactProps
  if (compactEvents !== undefined) spec.events = compactEvents
  if (children !== undefined && children.length > 0) spec.children = children
  return spec
}

function compactRecord(record) {
  if (record === undefined) return undefined
  const out = {}
  for (const [key, value] of Object.entries(record)) {
    if (value !== undefined) out[key] = value
  }
  return Object.keys(out).length > 0 ? out : undefined
}

function compactEventRecord(record) {
  if (record === undefined) return undefined
  const out = {}
  for (const [key, value] of Object.entries(record)) {
    if (value !== undefined) out[key] = value
  }
  return Object.keys(out).length > 0 ? out : undefined
}

function events(base, aliases) {
  const merged = { ...(base ?? {}) }
  for (const [key, value] of Object.entries(aliases)) {
    if (value !== undefined) merged[key] = value
  }
  return compactEventRecord(merged)
}

function enabledValue(options) {
  if (options.enabled !== undefined) return options.enabled
  if (options.disabled !== undefined) return !options.disabled
  return undefined
}

function containerArgs(first, second) {
  if (Array.isArray(first)) {
    return { options: {}, children: first }
  }
  const options = first ?? {}
  return { options, children: second ?? options.children }
}

function stackProps(options) {
  return {
    spacing: options.spacing,
    padding: options.padding,
    scrollable: options.scrollable,
  }
}

function typeAheadProps(options) {
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

function typeAheadEvents(options) {
  return events(options.events, {
    change: options.onChange,
    accept: options.onAccept,
    close: options.onClose,
  })
}

function isEmptyRecord(record) {
  return record === undefined || Object.keys(record).length === 0
}

module.exports = {
  component,
  child,
  withLayout,
  withMeta,
  tab,
  Text,
  Label,
  Button,
  TextBox,
  TextArea,
  Checkbox,
  RadioGroup,
  Slider,
  Spinner,
  ProgressBar,
  ListBox,
  TableView,
  VStack,
  HStack,
  Grid,
  Border,
  Visibility,
  Divider,
  Spacer,
  Splitter,
  TabView,
  TextSpan,
  RichText,
  StyledLabel,
  Disclosure,
  TypeAhead,
  CommandPalette,
}
