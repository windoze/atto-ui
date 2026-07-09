import {
  ChatApprovalOption,
  ChatApprovalRequest,
  ChatCompactBlock,
  Button,
  ChatInputPanel,
  ChatMentionCandidate,
  ChatMessageList,
  ChatPanel,
  ChatPlanBlock,
  ChatSlashCommand,
  ChatTaskBlock,
  ChatTaskTranscriptItem,
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
  useChatMessages,
  type ChatApprovalOption as ReactChatApprovalOption,
  type ChatApprovalRequest as ReactChatApprovalRequest,
} from '../src'

const button = <Button onClick={() => {}}>Save</Button>
const textbox = <TextBox value="Ada" onChange={(value, event) => {
  value.toUpperCase()
  event.targetId?.toString()
}} />
const list = <ListBox items={['one', 'two']} selectedIndex={0} onSelect={(index) => index.toFixed()} />
const table = <Table headers={['name']} rows={[["Ada"], ["Grace"]]} onChange={(index) => index.toFixed()} />
const approval = ChatApprovalRequest('approval-1', 'Run command?', [
  ChatApprovalOption('allow_project', 'Allow project', { action: 'allow', level: 'project' }),
])
const typedApprovalOption: ReactChatApprovalOption = approval.options[0]
const typedApprovalRequest: ReactChatApprovalRequest = approval
const chat = <ChatMessageList messages={[ChatTextMessage(1, 'hello', { role: 'user' }), { id: 2, role: 'assistant', status: 'complete', blocks: [ChatPlanBlock(2001, [{ text: 'plan' }]), ChatTaskBlock(2002, 'subagent', { transcript: [ChatTaskTranscriptItem('assistant', [ChatTextMessage(20, 'nested').blocks[0]])] }), ChatCompactBlock(2003, 'complete', { beforeTokens: 1000, afterTokens: 500, summary: 'summary' })] }]} autoScroll onLoadMore={() => {}} onApprove={(event) => event.payload} onEditDecision={(event) => event.payload} onPlanDecision={(event) => event.payload} onCancel={(event) => event.payload} onMessageAction={(event) => event.payload} />
const chatFill = <ChatMessageList messages={[ChatTextMessage(4, 'wide')]} fillWidth />
const chatRatio = <ChatMessageList messages={[ChatTextMessage(5, 'two thirds')]} bubbleWidthPercent={66} />
const inputPanel = <ChatInputPanel
  mode={{ kind: 'choice', title: 'Pick', options: ['a', 'b'], allowCustom: true }}
  draft=""
  slashCommands={[ChatSlashCommand('/clear', { id: 'clear', action: 'submit' })]}
  mentionCandidates={[ChatMentionCandidate('src/lib.rs')]}
  clearOnSubmit
  onSubmit={(event) => event.payload}
  onSlashCommand={(command) => command.action}
  onMentionQuery={(context) => context.query.toUpperCase()}
/>
const chatPanel = <ChatPanel list={{ messages: [ChatTextMessage(3, 'hi')], onMessageAction: () => {} }} input={{ mode: { kind: 'text' }, onSubmit: () => {} }} spacing={1} />
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
const rawInput = <chatInputPanel slash_commands={[ChatSlashCommand('/model')]} mention_candidates={[ChatMentionCandidate('Cargo.toml')]} onSlash_command={(event) => event.payload} onMention_query={(event) => event.payload} />
const rawChat = <chatMessageList messages={[ChatTextMessage(2, 'raw')]} onLoad_more={(event) => event.callbackId} onPlan_decision={(event) => event.payload} />
const menuEvent = <MenuBar><Menu title="File"><MenuItem label="Open" onClick={(event) => event.callbackId.toUpperCase()} /></Menu></MenuBar>
function ChatHookProbe() {
  const store = useChatMessages([{ id: 1, role: 'assistant', status: 'complete', blocks: [{ type: 'tool_use', block_id: 1001, call_id: 'call-1', name: 'bash', input: { text: '' }, status: 'pending', approval }] }])
  const { messageId, blockId } = store.addTextTurn('assistant', '', { status: 'streaming' })
  store.appendTextDelta(blockId, 'hi')
  store.setTurnStatus(messageId, 'complete')
  store.upsertToolResult('call-1', { ok: true, output: { ansi: 'done' } })
  store.resolveApproval(1001, 'allow_project')
  return <ChatMessageList messages={store.messages} />
}

void desktop
void rawTextBox
void rawList
void rawGrid
void rawInput
void rawChat
void menuEvent
void inputPanel
void chatPanel
void typedApprovalOption
void typedApprovalRequest
void chatFill
void chatRatio
void ChatHookProbe

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
