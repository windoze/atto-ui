use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use ratatui::layout::Rect;

use atto_ui::app::{
    AppControl, CrosstermAppConfig, Desktop, MenuBar, MenuItem, MenuSpec, run_crossterm_desktop,
};
use atto_ui::reactive::EventQueue;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};

use atto_ui_chat::{
    ApprovalDecision, ApprovalOption, ApprovalRequest, Artifact, ArtifactBlock, ArtifactId,
    ArtifactKind, ArtifactViewer, AttachmentBlock, ChatBlock, ChatBlockId, ChatChoiceInputConfig,
    ChatConfirmInputConfig, ChatError, ChatErrorKind, ChatInputHandle, ChatInputMode,
    ChatInputResponse, ChatMessage, ChatMessageId, ChatMessageList, ChatMessageMeta,
    ChatMessageStore, ChatPanel, ChatRole, ChatTurnStatus, DiffBlock, DiffData, EditDecision,
    EditDecisionEvent, MessageAction, MessageActionKind, NoticeBlock, NoticeLevel, PlanBlock,
    PlanDecision, PlanDecisionEvent, PlanItem, StopReason, TaskBlock, TaskStatus,
    TaskTranscriptItem, TextArtifactViewer, TextBlock, ThinkingBlock, TodoBlock, TodoItem,
    TodoState, TokenUsage, ToolInput, ToolOutput, ToolResultBlock, ToolStatus, ToolUseBlock,
};

/// 由菜单 / slash 命令触发，注入一段演示用的对话片段。
#[derive(Clone, Copy)]
enum DemoAction {
    Plan,
    Diff,
    Todo,
    Task,
    Approval,
    Error,
    Notices,
}

type Artifacts = Arc<Mutex<HashMap<ArtifactId, Artifact>>>;

fn main() -> Result<()> {
    let store = ChatMessageStore::new();
    let input_handle = ChatInputHandle::new();
    let artifacts: Artifacts = Arc::new(Mutex::new(HashMap::new()));
    let ai = MockAiServer::new(store.clone());

    seed_conversation(&store, &artifacts);

    // 列表回调与桌面层之间通信用的事件队列。
    let open_artifacts: EventQueue<ArtifactId> = EventQueue::new();
    let demo_actions: EventQueue<DemoAction> = EventQueue::new();

    let history_counter = Arc::new(AtomicU64::new(0));
    let list = ChatMessageList::new(store.clone())
        .wrap_width(60)
        .on_open_artifact({
            let open_artifacts = open_artifacts.clone();
            move |id| open_artifacts.push(id)
        })
        .on_approve({
            let store = store.clone();
            move |decision: ApprovalDecision| {
                store.resolve_approval(decision.block_id, decision.option_id.clone());
                push_system(
                    &store,
                    format!(
                        "审批结果：{} → {}",
                        decision.approval_id, decision.option_id
                    ),
                );
            }
        })
        .on_edit_decision({
            let store = store.clone();
            move |event: EditDecisionEvent| {
                store.set_edit_decision(event.block_id, event.decision);
                push_system(&store, format!("Diff {}", decision_label(event.decision)));
            }
        })
        .on_plan_decision({
            let store = store.clone();
            move |event: PlanDecisionEvent| {
                store.set_plan_decision(event.block_id, event.decision);
                push_system(&store, format!("Plan {}", plan_label(event.decision)));
            }
        })
        .on_message_action({
            let store = store.clone();
            let ai = ai.clone();
            move |action: MessageAction| handle_message_action(&store, &ai, action)
        })
        .on_cancel({
            let store = store.clone();
            move |message_id: ChatMessageId| {
                store.set_turn_status(message_id, ChatTurnStatus::Canceled);
                push_system(&store, "已中断生成");
            }
        })
        .on_load_more({
            let store = store.clone();
            let counter = history_counter.clone();
            move || {
                let page = counter.fetch_add(1, Ordering::Relaxed) + 1;
                let mut older = Vec::new();
                for idx in 0..3u64 {
                    let id = store.next_message_id();
                    let role = if idx % 2 == 0 {
                        ChatRole::User
                    } else {
                        ChatRole::Assistant
                    };
                    let message =
                        ChatMessage::text(id, role, format!("历史记录 {page} · 第 {idx} 条"))
                            .with_timestamp(format!("History {page}"));
                    older.push(message);
                }
                store.prepend_many(older);
            }
        });

    let input_panel = input_handle.panel().on_submit({
        let store = store.clone();
        let ai = ai.clone();
        let input_handle = input_handle.clone();
        let demo_actions = demo_actions.clone();
        move |resp| match resp {
            ChatInputResponse::Text(text) => {
                if handle_command(&input_handle, &demo_actions, &text) {
                    return;
                }
                push_user(&store, text.clone());
                ai.respond(text);
            }
            ChatInputResponse::Choice { index, label } => {
                input_handle.set_mode(default_text_mode());
                push_user(&store, format!("选择：{label} (#{index})"));
                ai.respond(label);
            }
            ChatInputResponse::Custom(text) => {
                input_handle.set_mode(default_text_mode());
                push_user(&store, text.clone());
                ai.respond(text);
            }
        }
    });

    let panel = ChatPanel::new(list, input_panel);

    let quit: EventQueue<()> = EventQueue::new();
    let menu = MenuBar::new(vec![
        MenuSpec::new(
            "File",
            vec![
                MenuItem::action("Quit", {
                    let quit = quit.clone();
                    move || quit.push(())
                })
                .shortcut("q"),
            ],
        ),
        MenuSpec::new(
            "Demo",
            vec![
                demo_menu_item("Plan 模式", DemoAction::Plan, &demo_actions),
                demo_menu_item("Inline Diff", DemoAction::Diff, &demo_actions),
                demo_menu_item("Todo 面板", DemoAction::Todo, &demo_actions),
                demo_menu_item("子 Agent 任务", DemoAction::Task, &demo_actions),
                demo_menu_item("工具审批", DemoAction::Approval, &demo_actions),
                demo_menu_item("错误回合", DemoAction::Error, &demo_actions),
                demo_menu_item("系统通知", DemoAction::Notices, &demo_actions),
            ],
        ),
    ]);

    let config = CrosstermAppConfig::default().tick_rate(Duration::from_millis(50));
    run_crossterm_desktop(
        config,
        move |screen: Rect| {
            let mut desktop = Desktop::new(Theme::dark(), menu.clone());
            let work = Desktop::layout(screen).work_area;
            let window = Window::new(
                WindowKind::Normal,
                "Chat Demo",
                Rect {
                    x: work.x.saturating_add(2),
                    y: work.y.saturating_add(1),
                    width: work.width.saturating_sub(4).max(48),
                    height: work.height.saturating_sub(3).max(18),
                },
                Box::new(panel),
            );
            desktop.add_window(window, screen);
            Ok(desktop)
        },
        move |desktop, screen| {
            for action in demo_actions.drain() {
                run_demo_action(&store, action);
            }
            for id in open_artifacts.drain() {
                let artifact = artifacts.lock().expect("artifacts lock").get(&id).cloned();
                if let Some(artifact) = artifact {
                    TextArtifactViewer::new(desktop, screen).open(artifact);
                }
            }
            if quit.pop().is_some() {
                return Ok(AppControl::Exit);
            }
            Ok(AppControl::Continue)
        },
        |_desktop, _event, _screen, _result| Ok(AppControl::Continue),
    )
}

fn demo_menu_item(label: &str, action: DemoAction, queue: &EventQueue<DemoAction>) -> MenuItem {
    let queue = queue.clone();
    MenuItem::action(label, move || queue.push(action))
}

fn default_text_mode() -> ChatInputMode {
    ChatInputMode::text("Message", Some("输入消息，或试试 /help".into()))
}

/// 解析 slash 命令；返回 true 表示已处理（不再当作普通消息发给 AI）。
fn handle_command(
    handle: &ChatInputHandle,
    demo_actions: &EventQueue<DemoAction>,
    text: &str,
) -> bool {
    let cmd = text.trim();
    match cmd.to_ascii_lowercase().as_str() {
        "/help" => {
            handle.set_mode(ChatInputMode::Choice(
                ChatChoiceInputConfig::new(
                    "可用演示（也可在 Demo 菜单中选择）",
                    vec![
                        "/plan  Plan 模式".into(),
                        "/diff  Inline Diff".into(),
                        "/todo  Todo 面板".into(),
                        "/task  子 Agent".into(),
                        "/approve 工具审批".into(),
                        "/error 错误回合".into(),
                    ],
                )
                .allow_custom(true),
            ));
            true
        }
        "/confirm" => {
            handle.set_mode(ChatInputMode::Confirm(
                ChatConfirmInputConfig::new("是否继续执行?")
                    .yes_label("继续")
                    .no_label("停止")
                    .allow_custom(true),
            ));
            true
        }
        "/choices" => {
            handle.set_mode(ChatInputMode::Choice(
                ChatChoiceInputConfig::new(
                    "请选择一种回应方式",
                    vec!["简短回复".into(), "详细解释".into(), "给出示例".into()],
                )
                .allow_custom(true)
                .submit_label("发送"),
            ));
            true
        }
        "/text" => {
            handle.set_mode(default_text_mode());
            true
        }
        "/plan" => push_demo(demo_actions, DemoAction::Plan),
        "/diff" => push_demo(demo_actions, DemoAction::Diff),
        "/todo" => push_demo(demo_actions, DemoAction::Todo),
        "/task" => push_demo(demo_actions, DemoAction::Task),
        "/approve" => push_demo(demo_actions, DemoAction::Approval),
        "/error" => push_demo(demo_actions, DemoAction::Error),
        "/notice" => push_demo(demo_actions, DemoAction::Notices),
        _ => false,
    }
}

fn push_demo(queue: &EventQueue<DemoAction>, action: DemoAction) -> bool {
    queue.push(action);
    true
}

fn run_demo_action(store: &ChatMessageStore, action: DemoAction) {
    match action {
        DemoAction::Plan => seed_plan(store),
        DemoAction::Diff => seed_diff(store),
        DemoAction::Todo => seed_todo(store),
        DemoAction::Task => seed_task(store),
        DemoAction::Approval => seed_approval(store),
        DemoAction::Error => seed_error_turn(store),
        DemoAction::Notices => seed_notices(store),
    }
}

fn handle_message_action(store: &ChatMessageStore, ai: &MockAiServer, action: MessageAction) {
    match action.kind {
        MessageActionKind::Copy | MessageActionKind::CopyBlock(_) => {
            push_system(store, "已复制到剪贴板");
        }
        MessageActionKind::Retry | MessageActionKind::Regenerate => {
            push_system(store, "重新生成回复…");
            ai.respond("请再试一次".to_string());
        }
        MessageActionKind::EditUser => {
            push_system(store, "编辑并重发：请在输入框修改后再次发送");
        }
    }
}

// ---- 初始对话：静态展示多种 block ----

fn seed_conversation(store: &ChatMessageStore, artifacts: &Artifacts) {
    store.push(
        ChatMessage::text(
            store.next_message_id(),
            ChatRole::System,
            "欢迎使用 Atto Chat。这是一个 agent 会话视图：一条消息可包含思考、工具调用、diff、todo 等多种内容块。\n\n输入 **/help** 查看可演示的能力，或直接发消息看多 block 流式回复。",
        )
        .with_timestamp("Boot"),
    );

    push_user(store, "帮我看看项目里有没有 TODO，再总结一下。".to_string());

    // 一条 assistant 回合，含思考 + 文本 + 工具调用 + 工具结果，并带回合元数据。
    let id = store.next_message_id();
    let mut message = ChatMessage::new(
        id,
        ChatRole::Assistant,
        vec![
            ChatBlock::Thinking(ThinkingBlock {
                id: block_id(id, 0),
                markdown: "先用 ripgrep 搜索 TODO 标记，再按文件归类。".to_string(),
                streaming: false,
                collapsed: true,
            }),
            ChatBlock::Text(TextBlock {
                id: block_id(id, 1),
                markdown: "我先在仓库里搜索 `TODO`：".to_string(),
                streaming: false,
            }),
            ChatBlock::ToolUse(ToolUseBlock {
                id: block_id(id, 2),
                call_id: "rg-todo".to_string(),
                name: "bash".to_string(),
                input: ToolInput::Text("rg -n \"TODO\" src/".to_string()),
                status: ToolStatus::Done,
                approval: None,
                collapsed: false,
            }),
            ChatBlock::ToolResult(ToolResultBlock {
                id: block_id(id, 3),
                call_id: "rg-todo".to_string(),
                ok: true,
                exit_code: Some(0),
                output: ToolOutput::Ansi(
                    "src/list.rs:412: // TODO: 虚拟化超长会话\nsrc/store.rs:88:  // TODO: 增量持久化"
                        .to_string(),
                ),
                collapsed: false,
            }),
            ChatBlock::Text(TextBlock {
                id: block_id(id, 4),
                markdown: "共找到 **2 处** TODO，主要集中在 `list.rs` 与 `store.rs`。"
                    .to_string(),
                streaming: false,
            }),
        ],
    )
    .with_timestamp("T+0s");
    message.meta = ChatMessageMeta {
        timestamp: Some("T+0s".to_string()),
        model: Some("claude-demo".to_string()),
        usage: Some(TokenUsage {
            input: 642,
            output: 88,
        }),
        elapsed_ms: Some(1240),
        stop_reason: Some(StopReason::EndTurn),
    };
    store.push(message);

    // 附件 + artifact 链接（可点开在独立窗口查看）。
    let code_id = ArtifactId::new("code-greet");
    let attach_id = store.next_message_id();
    store.push(ChatMessage::new(
        attach_id,
        ChatRole::Assistant,
        vec![
            ChatBlock::Attachment(AttachmentBlock {
                id: block_id(attach_id, 0),
                name: "summary.md".to_string(),
                url: Some("file:///tmp/summary.md".to_string()),
                mime: Some("text/markdown".to_string()),
            }),
            ChatBlock::Artifact(ArtifactBlock {
                id: block_id(attach_id, 1),
                kind: ArtifactKind::Code,
                anchor: code_id.clone(),
                title: "greet.rs".to_string(),
            }),
        ],
    ));

    let mut map = artifacts.lock().expect("artifacts lock");
    map.insert(
        code_id.clone(),
        Artifact::new(
            code_id,
            ArtifactKind::Code,
            "greet.rs",
            "fn greet(name: &str) {\n    println!(\"Hello, {name}!\");\n}",
        ),
    );
}

// ---- 各演示场景的 seed ----

fn seed_plan(store: &ChatMessageStore) {
    let id = store.next_message_id();
    store.push(ChatMessage::new(
        id,
        ChatRole::Assistant,
        vec![ChatBlock::Plan(PlanBlock {
            id: block_id(id, 0),
            items: vec![
                PlanItem {
                    text: "梳理现有 chat 数据模型".to_string(),
                },
                PlanItem {
                    text: "实现 block 渲染与流式更新".to_string(),
                },
                PlanItem {
                    text: "补充 PTY 快照测试".to_string(),
                },
            ],
            decision: PlanDecision::Pending,
        })],
    ));
    push_system(
        store,
        "Plan 模式：按 Accept / Reject 做决策（结果会回写）。",
    );
}

fn seed_diff(store: &ChatMessageStore) {
    let id = store.next_message_id();
    store.push(ChatMessage::new(
        id,
        ChatRole::Assistant,
        vec![ChatBlock::Diff(DiffBlock {
            id: block_id(id, 0),
            path: "src/main.rs".to_string(),
            diff: DiffData {
                unified: "@@ -1,3 +1,3 @@\n fn main() {\n-    println!(\"hi\");\n+    println!(\"hello, world\");\n }"
                    .to_string(),
            },
            decision: EditDecision::Pending,
        })],
    ));
    push_system(store, "Inline Diff：可 Accept / Reject，决策后锁定。");
}

fn seed_todo(store: &ChatMessageStore) {
    let id = store.next_message_id();
    let todo_id = block_id(id, 0);
    store.push(ChatMessage::new(
        id,
        ChatRole::Assistant,
        vec![ChatBlock::Todo(TodoBlock {
            id: todo_id,
            items: vec![
                TodoItem {
                    text: "设计 block 模型".to_string(),
                    state: TodoState::Done,
                },
                TodoItem {
                    text: "渲染各类 block".to_string(),
                    state: TodoState::InProgress,
                },
                TodoItem {
                    text: "编写测试".to_string(),
                    state: TodoState::Pending,
                },
            ],
        })],
    ));

    // 模拟进度推进。
    let store = store.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(1500));
        store.set_todo(
            todo_id,
            vec![
                TodoItem {
                    text: "设计 block 模型".to_string(),
                    state: TodoState::Done,
                },
                TodoItem {
                    text: "渲染各类 block".to_string(),
                    state: TodoState::Done,
                },
                TodoItem {
                    text: "编写测试".to_string(),
                    state: TodoState::InProgress,
                },
            ],
        );
    });
}

fn seed_task(store: &ChatMessageStore) {
    let id = store.next_message_id();
    let task_id = block_id(id, 0);
    store.push(ChatMessage::new(
        id,
        ChatRole::Assistant,
        vec![ChatBlock::Task(TaskBlock {
            id: task_id,
            title: "子 Agent：搜索用法".to_string(),
            status: TaskStatus::Running,
            summary: "正在检索 ChatMessageList 的调用点…".to_string(),
            transcript: vec![TaskTranscriptItem {
                role: ChatRole::Assistant,
                blocks: vec![ChatBlock::ToolUse(ToolUseBlock {
                    id: ChatBlockId::new(id.0.saturating_mul(1_000).saturating_add(900)),
                    call_id: "nested-grep".to_string(),
                    name: "grep".to_string(),
                    input: ToolInput::Text("rg ChatMessageList".to_string()),
                    status: ToolStatus::Running,
                    approval: None,
                    collapsed: false,
                })],
            }],
            collapsed: true,
        })],
    ));

    let store = store.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(1800));
        store.set_task_status(task_id, TaskStatus::Complete);
        store.set_task_summary(task_id, "找到 3 处调用，已汇总。");
        store.set_task_transcript(
            task_id,
            vec![TaskTranscriptItem {
                role: ChatRole::Assistant,
                blocks: vec![ChatBlock::Text(TextBlock {
                    id: ChatBlockId::new(task_id.0.saturating_add(1)),
                    markdown: "demo / snapshot_app / 集成测试 各 1 处。".to_string(),
                    streaming: false,
                })],
            }],
        );
    });
}

fn seed_approval(store: &ChatMessageStore) {
    let id = store.next_message_id();
    store.push(ChatMessage::new(
        id,
        ChatRole::Assistant,
        vec![ChatBlock::ToolUse(ToolUseBlock {
            id: block_id(id, 0),
            call_id: "approve-rm".to_string(),
            name: "bash".to_string(),
            input: ToolInput::Text("rm -rf target/".to_string()),
            status: ToolStatus::Pending,
            approval: Some(ApprovalRequest {
                id: "approval-rm".to_string(),
                prompt: "是否允许执行 `rm -rf target/`?".to_string(),
                options: vec![
                    ApprovalOption::allow_once("allow_once", "仅此一次"),
                    ApprovalOption::allow_always("allow_always", "总是允许"),
                    ApprovalOption::deny("deny", "拒绝"),
                ],
                resolved: None,
            }),
            collapsed: false,
        })],
    ));
}

fn seed_error_turn(store: &ChatMessageStore) {
    let id = store.next_message_id();
    let mut message = ChatMessage::text(id, ChatRole::Assistant, "请求上游模型时出错。")
        .with_status(ChatTurnStatus::Failed(
            ChatError::new(ChatErrorKind::RateLimit, "触发速率限制 (429)")
                .with_detail("retry-after: 30s"),
        ));
    message.meta = ChatMessageMeta {
        timestamp: Some(now_label()),
        model: Some("claude-demo".to_string()),
        usage: Some(TokenUsage {
            input: 120,
            output: 0,
        }),
        elapsed_ms: Some(310),
        stop_reason: Some(StopReason::Refusal),
    };
    store.push(message);
}

fn seed_notices(store: &ChatMessageStore) {
    let id = store.next_message_id();
    store.push(ChatMessage::new(
        id,
        ChatRole::System,
        vec![
            ChatBlock::Notice(NoticeBlock {
                id: block_id(id, 0),
                level: NoticeLevel::Info,
                text: "已加载项目上下文（54 个源文件）。".to_string(),
            }),
            ChatBlock::Notice(NoticeBlock {
                id: block_id(id, 1),
                level: NoticeLevel::Warning,
                text: "上下文接近上限，已压缩较早的消息。".to_string(),
            }),
            ChatBlock::Notice(NoticeBlock {
                id: block_id(id, 2),
                level: NoticeLevel::Error,
                text: "一个后台工具调用超时。".to_string(),
            }),
        ],
    ));
}

// ---- 通用 helper ----

fn push_user(store: &ChatMessageStore, text: String) {
    let id = store.next_message_id();
    store.push(ChatMessage::text(id, ChatRole::User, text).with_timestamp(now_label()));
}

fn push_system(store: &ChatMessageStore, text: impl Into<String>) {
    let id = store.next_message_id();
    store.push(ChatMessage::text(id, ChatRole::System, text.into()));
}

fn decision_label(decision: EditDecision) -> &'static str {
    match decision {
        EditDecision::Accepted => "已接受",
        EditDecision::Rejected => "已拒绝",
        EditDecision::Pending => "待定",
    }
}

fn plan_label(decision: PlanDecision) -> &'static str {
    match decision {
        PlanDecision::Accepted => "已接受",
        PlanDecision::Rejected => "已拒绝",
        PlanDecision::Pending => "待定",
    }
}

/// 与 message.rs 中的派生规则一致：保证同一条消息内各 block id 唯一。
fn block_id(message_id: ChatMessageId, ordinal: u64) -> ChatBlockId {
    ChatBlockId::new(
        message_id
            .0
            .saturating_mul(1_000)
            .saturating_add(ordinal + 1),
    )
}

// ---- mock AI：对用户输入回复一段多 block 流式回合 ----

#[derive(Clone)]
struct MockAiServer {
    store: ChatMessageStore,
    rng: Arc<Mutex<XorShift64>>,
}

impl MockAiServer {
    fn new(store: ChatMessageStore) -> Self {
        Self {
            store,
            rng: Arc::new(Mutex::new(XorShift64::new(now_seed()))),
        }
    }

    fn respond(&self, prompt: String) {
        let store = self.store.clone();
        let with_tool = {
            let mut rng = self.rng.lock().expect("rng lock");
            rng.gen_range(0, 100) < 60
        };
        thread::spawn(move || stream_turn(&store, &prompt, with_tool));
    }
}

/// 流式生成一条 assistant 回合：思考 → 文本（→ 可选工具调用+结果）→ 元数据。
fn stream_turn(store: &ChatMessageStore, prompt: &str, with_tool: bool) {
    let id = store.next_message_id();
    let thinking = block_id(id, 0);
    let answer = block_id(id, 1);

    let mut blocks = vec![
        ChatBlock::Thinking(ThinkingBlock {
            id: thinking,
            markdown: String::new(),
            streaming: true,
            collapsed: false,
        }),
        ChatBlock::Text(TextBlock {
            id: answer,
            markdown: String::new(),
            streaming: true,
        }),
    ];

    let tool_use = block_id(id, 2);
    let tool_result = block_id(id, 3);
    if with_tool {
        blocks.push(ChatBlock::ToolUse(ToolUseBlock {
            id: tool_use,
            call_id: format!("call-{}", id.0),
            name: "bash".to_string(),
            input: ToolInput::Text("echo \"working...\"".to_string()),
            status: ToolStatus::Running,
            approval: None,
            collapsed: false,
        }));
        blocks.push(ChatBlock::ToolResult(ToolResultBlock {
            id: tool_result,
            call_id: format!("call-{}", id.0),
            ok: true,
            exit_code: Some(0),
            output: ToolOutput::Ansi(String::new()),
            collapsed: false,
        }));
    }

    let message = ChatMessage::new(id, ChatRole::Assistant, blocks)
        .with_status(ChatTurnStatus::Streaming)
        .with_timestamp(now_label());
    store.push(message);

    stream_into(store, thinking, "让我先理解你的意图，再组织回复。");

    if with_tool {
        stream_into(store, tool_result, "working...\ndone.");
        store.set_tool_status(tool_use, ToolStatus::Done);
    }

    let reply = format!(
        "收到，你说的是：「{}」。\n\n这是一段 **Markdown** 回复：\n- 支持列表\n- 支持 `code`\n\n```rust\nfn demo() {{\n    println!(\"streamed\");\n}}\n```",
        prompt.trim()
    );
    stream_into(store, answer, &reply);

    store.set_turn_status(id, ChatTurnStatus::Complete);
    store.set_meta(
        id,
        ChatMessageMeta {
            timestamp: Some(now_label()),
            model: Some("claude-demo".to_string()),
            usage: Some(TokenUsage {
                input: 320 + (prompt.len() as u64),
                output: reply.len() as u64,
            }),
            elapsed_ms: Some(900),
            stop_reason: Some(StopReason::EndTurn),
        },
    );
}

/// 把一段文本按小块流式写入指定 block（文本/思考用 delta，工具结果用 output delta）。
fn stream_into(store: &ChatMessageStore, block: ChatBlockId, full: &str) {
    let is_tool = store
        .with_block(block, |b| matches!(b, ChatBlock::ToolResult(_)))
        .unwrap_or(false);
    let chars: Vec<char> = full.chars().collect();
    let mut idx = 0usize;
    let mut rng = XorShift64::new(now_seed());
    while idx < chars.len() {
        let step = rng.gen_range(1, 4) as usize;
        let end = (idx + step).min(chars.len());
        let delta: String = chars[idx..end].iter().collect();
        if is_tool {
            store.append_tool_output(block, &delta);
        } else {
            store.append_text_delta(block, &delta);
        }
        idx = end;
        thread::sleep(Duration::from_millis(rng.gen_range(60, 160)));
    }
}

fn now_label() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs();
    format!("T+{}s", secs % 100000)
}

fn now_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(1))
        .as_nanos() as u64
}

#[derive(Clone, Debug)]
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        let seed = if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        };
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn gen_range(&mut self, min: u64, max: u64) -> u64 {
        if min >= max {
            return min;
        }
        let span = max - min + 1;
        min + (self.next_u64() % span)
    }
}
