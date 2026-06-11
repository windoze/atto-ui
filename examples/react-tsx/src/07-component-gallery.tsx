/**
 * 07 — Component gallery
 *
 * One desktop, several windows, each showing a group of components:
 *   Controls (Button/Checkbox/RadioGroup/Slider/ProgressBar/Spinner/TextBox/TextArea/Label),
 *   Layout (HStack/Grid/Border/Divider/Disclosure), Data (ListBox/Table),
 *   File Tree (FileTree with colored file-type icons), Markdown, and Editor.
 *
 * Window lifecycle (the binding/React feature this demo exercises):
 *   - The "Demos" menu re-creates a window after it was closed.
 *   - Closing a window from the TUI titlebar fires <Window onClose>, which
 *     unmounts it; "Demos" → "Show X" mounts a fresh one.
 *   - The "Window" menu uses <WindowOpMenuItem>: predefined-id items
 *     (cascade/tile/minimize/maximize/…) that the runtime performs natively,
 *     with no onClick wiring. Minimizing from the TUI fires <Window onMinimize>;
 *     "Restore all" brings minimized windows back.
 *   - "Window" → "Minimized windows" is a <MinimizedWindowsMenu>: the native
 *     runtime fills it with the minimized windows and restores the one picked.
 *
 * Run interactively:  npm run gallery   (F10 menus, Ctrl+W window mode, Ctrl+Q quit)
 * Headless smoke:      ATTO_UI_EXAMPLE_HEADLESS=1 npm run gallery
 */
import { useState } from 'react'
import {
  Border,
  Button,
  Checkbox,
  Disclosure,
  Divider,
  Editor,
  FileTree,
  Grid,
  HStack,
  Label,
  ListBox,
  Markdown,
  Menu,
  MenuBar,
  MenuItem,
  MinimizedWindowsMenu,
  ProgressBar,
  RadioGroup,
  Slider,
  Spinner,
  StatusBar,
  Table,
  TextArea,
  TextBox,
  VStack,
  Window,
  WindowOpMenuItem,
} from '@atto-ui/react'

import { startDemo, waitFor, hasText } from './_runtime'

type WindowKey = 'controls' | 'layout' | 'data' | 'filetree' | 'markdown' | 'editor'

const WINDOWS: ReadonlyArray<{ key: WindowKey; title: string; rect: [number, number, number, number] }> = [
  { key: 'controls', title: 'Controls', rect: [1, 1, 40, 21] },
  { key: 'layout', title: 'Layout', rect: [38, 1, 42, 21] },
  { key: 'data', title: 'Data', rect: [2, 4, 40, 14] },
  { key: 'markdown', title: 'Markdown', rect: [38, 4, 40, 14] },
  { key: 'editor', title: 'Editor', rect: [8, 7, 64, 11] },
  { key: 'filetree', title: 'File Tree', rect: [4, 2, 36, 18] },
]

const FILE_TREE_NODES = [
  {
    id: 1,
    name: 'src',
    kind: 'directory',
    expanded: true,
    children: [
      { id: 2, name: 'main.rs' },
      { id: 3, name: 'lib.rs' },
    ],
  },
  { id: 4, name: 'assets', kind: 'directory', children: [{ id: 5, name: 'logo.png' }] },
  { id: 6, name: 'README.md' },
  { id: 7, name: 'Cargo.toml' },
] as const

// File-type icons are opt-in (empty by default). Short text glyphs keep this
// readable on terminals without Nerd Fonts; each carries its own color.
const FILE_TREE_ICONS = {
  rs: { glyph: 'rs', color: '#dd6644' },
  md: { glyph: 'md', color: '#66aadd' },
  toml: { glyph: 'tm', color: '#aa88cc' },
  png: { glyph: 'im', color: '#66bb88' },
} as const

const MARKDOWN = `# Component Gallery

Rendered by **@atto-ui/react**.

- Live React state
- Multiple windows
- \`Markdown\` + \`Editor\`
`

const CODE = `fn main() {
    let items = ["button", "slider", "editor"];
    for (i, name) in items.iter().enumerate() {
        println!("{i}: {name}");
    }
}`

function App(): React.ReactElement {
  const [visible, setVisible] = useState<Record<WindowKey, boolean>>({
    controls: true,
    layout: true,
    data: true,
    filetree: true,
    markdown: true,
    editor: true,
  })
  const [log, setLog] = useState('ready')

  // Controls state
  const [checked, setChecked] = useState(true)
  const [radio, setRadio] = useState(1)
  const [slider, setSlider] = useState(40)
  const [name, setName] = useState('atto')
  const [notes, setNotes] = useState('multi-line\ntext area')
  const [fruit, setFruit] = useState(0)
  const [treeSelection, setTreeSelection] = useState<number | null>(2)

  const show = (key: WindowKey) => {
    setVisible((v) => ({ ...v, [key]: true }))
    setLog(`show ${key}`)
  }
  const onClose = (key: WindowKey) => () => {
    setVisible((v) => ({ ...v, [key]: false }))
    setLog(`closed ${key}`)
  }
  const onMinimize = (key: WindowKey) => () => setLog(`minimized ${key}`)
  const onRestore = (key: WindowKey) => () => setLog(`restored ${key}`)

  const windowChrome = (key: WindowKey) => ({
    onClose: onClose(key),
    onMinimize: onMinimize(key),
    onRestore: onRestore(key),
  })

  return (
    <>
      <MenuBar>
        <Menu title="Demos">
          {WINDOWS.map((w) => (
            <MenuItem key={w.key} label={`Show ${w.title}`} onClick={() => show(w.key)} />
          ))}
        </Menu>
        <Menu title="Window">
          {/*
           * Standard window operations. These carry predefined ids and run
           * natively in the runtime — no onClick wiring required. The
           * minimize/maximize/close ops act on the focused window.
           */}
          <WindowOpMenuItem op="cascade" />
          <WindowOpMenuItem op="tile" />
          <WindowOpMenuItem op="minimizeAll" />
          <WindowOpMenuItem op="restoreAll" />
          <WindowOpMenuItem op="minimize" shortcut="m" />
          <WindowOpMenuItem op="maximize" shortcut="x" />
          <WindowOpMenuItem op="close" shortcut="c" />
          <MinimizedWindowsMenu />
        </Menu>
      </MenuBar>

      {visible.controls && (
        <Window title="Controls" rect={WINDOWS[0].rect} {...windowChrome('controls')}>
          <VStack spacing={1} padding={1} scrollable>
            <Label text="Buttons & inputs" />
            <Button onClick={() => setLog('button: Push')}>Push</Button>
            <Checkbox label="Enable feature" checked={checked} onChange={setChecked} />
            <RadioGroup options={['Low', 'Medium', 'High']} selectedIndex={radio} onChange={setRadio} />
            <Slider min={0} max={100} value={slider} onChange={setSlider} />
            <ProgressBar min={0} max={100} value={slider} showText />
            <Spinner text="Working" running />
            <TextBox value={name} onChange={setName} placeholder="Name" />
            <TextArea value={notes} onChange={setNotes} height={3} />
          </VStack>
        </Window>
      )}

      {visible.layout && (
        <Window title="Layout" rect={WINDOWS[1].rect} {...windowChrome('layout')}>
          <VStack spacing={1} padding={1} scrollable>
            <Label text="HStack" />
            <HStack spacing={2}>
              <Label text="A" />
              <Label text="B" />
              <Label text="C" />
            </HStack>
            <Divider />
            <Label text="Grid 2x2" />
            <Grid columns={2} rowGap={1} columnGap={2}>
              <Label text="r1c1" />
              <Label text="r1c2" />
              <Label text="r2c1" />
              <Label text="r2c2" />
            </Grid>
            <Border border>
              <Label text="inside a Border" />
            </Border>
            <Disclosure title="Disclosure" expanded>
              <Label text="expanded content" />
            </Disclosure>
          </VStack>
        </Window>
      )}

      {visible.data && (
        <Window title="Data" rect={WINDOWS[2].rect} {...windowChrome('data')}>
          <VStack spacing={1} padding={1}>
            <Label text="Lists & tables" />
            <ListBox
              items={['Apple', 'Banana', 'Cherry']}
              selectedIndex={fruit}
              onSelect={setFruit}
              layout={{ height: 'fill' }}
            />
            <Table
              headers={['Name', 'Qty']}
              rows={[
                ['Apple', '3'],
                ['Pear', '5'],
                ['Plum', '2'],
              ]}
              layout={{ height: 'fill' }}
            />
          </VStack>
        </Window>
      )}

      {visible.filetree && (
        <Window title="File Tree" rect={WINDOWS[5].rect} {...windowChrome('filetree')}>
          <VStack spacing={1} padding={1}>
            <Label text="Project files" />
            <FileTree
              nodes={FILE_TREE_NODES}
              icons={FILE_TREE_ICONS}
              selection={treeSelection}
              onSelect={(id) => {
                setTreeSelection(id)
                setLog(id === null ? 'file tree: cleared' : `file tree: selected ${id}`)
              }}
              layout={{ height: 'fill' }}
            />
          </VStack>
        </Window>
      )}

      {visible.markdown && (
        <Window title="Markdown" rect={WINDOWS[3].rect} {...windowChrome('markdown')}>
          <VStack padding={1}>
            <Label text="Markdown docs" />
            <Markdown layout={{ height: 'fill' }}>{MARKDOWN}</Markdown>
          </VStack>
        </Window>
      )}

      {visible.editor && (
        <Window title="Editor" rect={WINDOWS[4].rect} {...windowChrome('editor')}>
          <VStack padding={1}>
            <Label text="Rust source" />
            <Editor value={CODE} languageId="rust" showLineNumbers layout={{ height: 'fill' }} />
          </VStack>
        </Window>
      )}

      <StatusBar left="Component Gallery — F10 menus, Ctrl+W window mode" right={log} />
    </>
  )
}

startDemo(<App />, {
  singleWindow: false,
  idPrefix: 'gallery',
  cols: 80,
  rows: 24,
  async headlessProbe(h) {
    await waitFor(() => h.windowIds().length === 6, 'six windows')
    await waitFor(() => hasText(h, 'Buttons & inputs'), 'controls window')
    await waitFor(() => hasText(h, 'Grid 2x2'), 'layout window')
    await waitFor(() => hasText(h, 'Lists & tables'), 'data window')
    await waitFor(() => hasText(h, 'Project files'), 'file tree window')
    await waitFor(() => hasText(h, 'Markdown docs'), 'markdown window')
    await waitFor(() => hasText(h, 'Rust source'), 'editor window')
  },
})
