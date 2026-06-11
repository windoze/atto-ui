import {
  Button,
  ChatMessageList,
  ChatPlanBlock,
  ChatTextMessage,
  Grid,
  HStack,
  ListBox,
  Menu,
  MenuBar,
  MenuItem,
  StatusBar,
  Table,
  TextBox,
  VStack,
  Window,
} from '../src'

const button = <Button onClick={() => {}}>Save</Button>
const textbox = <TextBox value="Ada" onChange={(value, event) => {
  value.toUpperCase()
  event.targetId?.toString()
}} />
const list = <ListBox items={['one', 'two']} selectedIndex={0} onSelect={(index) => index.toFixed()} />
const table = <Table headers={['name']} rows={[["Ada"], ["Grace"]]} onChange={(index) => index.toFixed()} />
const chat = <ChatMessageList messages={[ChatTextMessage(1, 'hello', { role: 'user' }), { id: 2, role: 'assistant', status: 'complete', blocks: [ChatPlanBlock(2001, [{ text: 'plan' }])] }]} autoScroll onLoadMore={() => {}} onPlanDecision={(event) => event.payload} />
const layout = <Grid columns={2} rowGap={1} columnGap={1}>{button}{textbox}</Grid>
const stack = <VStack spacing={1}><HStack>{layout}</HStack>{list}{table}{chat}</VStack>
const desktop = <>
  <MenuBar><Menu title="File"><MenuItem label="Open" onClick={() => {}} /></Menu></MenuBar>
  <StatusBar left="Ready" right="Ln 1" />
  <Window title="Main" rect={[1, 1, 40, 10]}>{stack}</Window>
</>

const rawTextBox = <textBox title="Raw" text="value" onChange={(event) => event.payload} />
const rawList = <listBox items={['one']} selection={0} onChange={(event) => event.callbackId} />
const rawGrid = <grid columns={2} row_gap={1} column_gap={1} />
const rawChat = <chatMessageList messages={[ChatTextMessage(2, 'raw')]} onLoad_more={(event) => event.callbackId} onPlan_decision={(event) => event.payload} />
const menuEvent = <MenuBar><Menu title="File"><MenuItem label="Open" onClick={(event) => event.callbackId.toUpperCase()} /></Menu></MenuBar>
void desktop
void rawTextBox
void rawList
void rawGrid
void rawChat
void menuEvent

// @ts-expect-error controlled TextBox requires value
const missingValue = <TextBox onChange={() => {}} />
void missingValue

// @ts-expect-error TextBox onChange receives the next string value first
const wrongChange = <TextBox value="" onChange={(value: number) => value.toFixed()} />
void wrongChange

// @ts-expect-error StatusBar is a fixed desktop slot and does not accept children
const statusBarChildren = <StatusBar>bad</StatusBar>
void statusBarChildren

// @ts-expect-error raw host TextBox uses the runtime text prop; value belongs to the wrapper
const rawValue = <textBox value="not supported" />
void rawValue

// @ts-expect-error raw host Grid uses runtime snake_case props; the Grid wrapper accepts camelCase
const rawGridCamelGap = <grid rowGap={1} />
void rawGridCamelGap
