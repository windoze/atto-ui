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
    ChatChoiceInputConfig, ChatConfirmInputConfig, ChatInputHandle, ChatInputMode,
    ChatInputResponse, ChatMessage, ChatMessageList, ChatMessageStatus, ChatMessageStore,
    ChatPanel, ChatSender,
};

const REPLIES: &[&str] = &[
    "这里是一段 **Markdown** 示例：\n\n- 支持列表\n- 支持粗体\n- 支持 `code`\n",
    "我可以帮你总结一下：\n\n1. 先收集输入\n2. 再生成输出\n3. 最后校验结果\n",
    "代码块示例：\n\n```rust\nfn greet() {\n    println!(\"hello\");\n}\n```\n",
    "收到。稍等我处理一下……",
    "如果需要更多上下文，请告诉我。",
];

fn main() -> Result<()> {
    let store = ChatMessageStore::new();
    let input_handle = ChatInputHandle::new();
    let ai = MockAiServer::new(store.clone());

    seed_messages(&store);

    let history_counter = Arc::new(AtomicU64::new(0));
    let list = ChatMessageList::new(store.binding())
        .wrap_width(72)
        .on_load_more({
            let store = store.clone();
            let counter = history_counter.clone();
            move || {
                let page = counter.fetch_add(1, Ordering::Relaxed) + 1;
                let mut older = Vec::new();
                for idx in 0..3u64 {
                    let id = store.next_message_id();
                    let text = format!("(历史记录 {page}) 上一段对话 #{idx}");
                    let message = ChatMessage::text(id, ChatSender::System, text)
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
        move |resp| match resp {
            ChatInputResponse::Text(text) => {
                if handle_command(&input_handle, &text) {
                    return;
                }
                push_user_message(&store, text.clone());
                ai.respond(text);
            }
            ChatInputResponse::Choice { index, label } => {
                let text = format!("选择：{label} (#{index})");
                input_handle.set_mode(default_text_mode());
                push_user_message(&store, text.clone());
                ai.respond(text);
            }
            ChatInputResponse::Custom(text) => {
                input_handle.set_mode(default_text_mode());
                push_user_message(&store, text.clone());
                ai.respond(text);
            }
        }
    });

    let panel = ChatPanel::new(list, input_panel);

    let actions: EventQueue<()> = EventQueue::new();
    let menu = MenuBar::new(vec![MenuSpec::new(
        "File",
        vec![
            MenuItem::action("Quit", {
                let actions = actions.clone();
                move || actions.push(())
            })
            .shortcut("q"),
        ],
    )]);

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
                    width: work.width.saturating_sub(4).max(40),
                    height: work.height.saturating_sub(3).max(15),
                },
                Box::new(panel),
            );
            desktop.add_window(window, screen);
            Ok(desktop)
        },
        move |_desktop, _screen| {
            if actions.pop().is_some() {
                return Ok(AppControl::Exit);
            }
            Ok(AppControl::Continue)
        },
        |_desktop, _event, _screen, _result| Ok(AppControl::Continue),
    )
}

fn default_text_mode() -> ChatInputMode {
    ChatInputMode::text("Message", Some("Type a message...".into()))
}

fn handle_command(handle: &ChatInputHandle, text: &str) -> bool {
    let cmd = text.trim();
    if cmd.eq_ignore_ascii_case("/confirm") {
        let mode = ChatConfirmInputConfig::new("是否继续执行?")
            .yes_label("继续")
            .no_label("停止")
            .allow_custom(true);
        handle.set_mode(ChatInputMode::Confirm(mode));
        return true;
    }
    if cmd.eq_ignore_ascii_case("/choices") {
        let mode = ChatChoiceInputConfig::new(
            "请选择一种回应方式",
            vec!["简短回复".into(), "详细解释".into(), "给出示例".into()],
        )
        .allow_custom(true)
        .submit_label("发送");
        handle.set_mode(ChatInputMode::Choice(mode));
        return true;
    }
    if cmd.eq_ignore_ascii_case("/text") {
        handle.set_mode(default_text_mode());
        return true;
    }
    false
}

fn push_user_message(store: &ChatMessageStore, text: String) {
    let id = store.next_message_id();
    let message = ChatMessage::text(id, ChatSender::User, text).with_timestamp(now_label());
    store.push(message);
}

fn seed_messages(store: &ChatMessageStore) {
    let welcome = ChatMessage::text(
        store.next_message_id(),
        ChatSender::System,
        "欢迎来到 ChatMessageList Demo。输入 /confirm 或 /choices 体验不同输入模式。",
    )
    .with_timestamp("Boot".to_string());
    store.push(welcome);
}

#[derive(Clone)]
struct MockAiServer {
    store: ChatMessageStore,
    rng: Arc<Mutex<XorShift64>>,
}

impl MockAiServer {
    fn new(store: ChatMessageStore) -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(1))
            .as_nanos() as u64;
        Self {
            store,
            rng: Arc::new(Mutex::new(XorShift64::new(seed))),
        }
    }

    fn respond(&self, prompt: String) {
        let store = self.store.clone();
        let rng = self.rng.clone();
        thread::spawn(move || {
            let (delay_ms, stream, file_reply, reply) = {
                let mut rng = rng.lock().expect("rng lock");
                let delay = rng.gen_range(600, 1500);
                let stream = rng.gen_range(0, 100) < 80;
                let file = rng.gen_range(0, 100) < 20;
                let reply = rng.pick_reply(&prompt);
                (delay, stream, file, reply)
            };

            thread::sleep(Duration::from_millis(delay_ms));

            if file_reply {
                let id = store.next_message_id();
                let message = ChatMessage::file(
                    id,
                    ChatSender::Assistant,
                    "report.txt",
                    Some("https://example.com/report.txt".to_string()),
                )
                .with_timestamp(now_label());
                store.push(message);
                return;
            }

            if stream {
                stream_reply(&store, reply);
            } else {
                let id = store.next_message_id();
                let message =
                    ChatMessage::text(id, ChatSender::Assistant, reply).with_timestamp(now_label());
                store.push(message);
            }
        });
    }
}

fn stream_reply(store: &ChatMessageStore, reply: String) {
    let id = store.next_message_id();
    let message = ChatMessage::text(id, ChatSender::Assistant, "")
        .with_status(ChatMessageStatus::InProgress)
        .with_timestamp(now_label());
    store.push(message);

    let chars: Vec<char> = reply.chars().collect();
    let mut acc = String::new();
    let mut idx = 0usize;
    let mut rng = XorShift64::new(now_seed());
    while idx < chars.len() {
        let step = rng.gen_range(1, 3) as usize;
        let end = (idx + step).min(chars.len());
        for ch in &chars[idx..end] {
            acc.push(*ch);
        }
        store.update_text(id, acc.clone());
        idx = end;
        let pause = rng.gen_range(120, 260);
        thread::sleep(Duration::from_millis(pause));
    }

    store.set_status(id, ChatMessageStatus::Final);
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

    fn pick_reply(&mut self, prompt: &str) -> String {
        let idx = self.gen_range(0, (REPLIES.len().saturating_sub(1)) as u64) as usize;
        let base = REPLIES.get(idx).unwrap_or(&"收到。");
        if prompt.trim().is_empty() {
            return base.to_string();
        }
        format!("{base}\n\n> 你刚才说：{prompt}")
    }
}
