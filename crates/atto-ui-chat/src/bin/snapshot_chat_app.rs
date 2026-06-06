use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;

use atto_ui::app::{Desktop, MenuBar, MenuItem, MenuSpec};
use atto_ui::reactive::EventQueue;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};

use atto_ui_chat::{
    ChatChoiceInputConfig, ChatConfirmInputConfig, ChatInputHandle, ChatInputMode,
    ChatInputResponse, ChatMessage, ChatMessageList, ChatMessageStatus, ChatMessageStore,
    ChatPanel, ChatSender, ChatToolCallStatus,
};

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        event::EnableMouseCapture,
        cursor::Show
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let actions: EventQueue<()> = EventQueue::new();
    let args: Vec<String> = std::env::args().collect();
    let streaming_markdown = args.iter().any(|arg| arg == "--streaming-markdown");
    let tool_call = args.iter().any(|arg| arg == "--tool-call");
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

    let store = ChatMessageStore::new();
    let tool_message_id = if tool_call {
        let id = store.next_message_id();
        store.push(ChatMessage::tool_call(
            id,
            "build",
            ChatToolCallStatus::Running,
            "TOOL-START",
        ));
        Some(id)
    } else {
        None
    };
    let streaming_message_id = if tool_message_id.is_some() {
        None
    } else if streaming_markdown {
        let id = store.next_message_id();
        store.push(
            ChatMessage::text(id, ChatSender::Assistant, "STREAMING-MARKDOWN")
                .with_status(ChatMessageStatus::InProgress),
        );
        Some(id)
    } else {
        seed_messages(&store, 28);
        None
    };

    let input_handle = ChatInputHandle::new();
    let load_counter = Arc::new(AtomicU64::new(0));
    let list = ChatMessageList::new(store.binding())
        .wrap_width(56)
        .show_timestamps(false)
        .on_load_more({
            let store = store.clone();
            let counter = load_counter.clone();
            move || {
                let page = counter.fetch_add(1, Ordering::Relaxed) + 1;
                let mut older = Vec::new();
                for idx in 0..3u64 {
                    let message = ChatMessage::text(
                        store.next_message_id(),
                        ChatSender::System,
                        format!("HISTORY-{page}-{idx}"),
                    );
                    older.push(message);
                }
                store.prepend_many(older);
            }
        });
    let input_panel = input_handle.panel().on_submit({
        let store = store.clone();
        move |response| {
            let text = submit_response_text(response);
            store.push(ChatMessage::text(
                store.next_message_id(),
                ChatSender::System,
                text,
            ));
        }
    });
    let panel = ChatPanel::new(list, input_panel);

    let mut desktop = Desktop::new(Theme::dark(), menu);
    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;

    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Chat Snapshot",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(2),
                width: 60.min(work.width.saturating_sub(2)).max(30),
                height: 16.min(work.height.saturating_sub(2)).max(10),
            },
            Box::new(panel),
        ),
        screen,
    );

    loop {
        terminal.draw(|f| desktop.draw(f))?;

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let ev = event::read()?;

        if let Event::Key(KeyEvent {
            code: KeyCode::Char(cmd),
            kind: KeyEventKind::Press,
            modifiers,
            ..
        }) = ev
            && modifiers.is_empty()
        {
            if let Some(id) = streaming_message_id {
                match cmd {
                    '1' => {
                        store.update_text(
                            id,
                            "```rust\nfn main() {\n    println!(\"STREAMING-CODE\");",
                        );
                        continue;
                    }
                    '2' => {
                        store.append_delta(id, "\n}\n```");
                        store.set_status(id, ChatMessageStatus::Final);
                        continue;
                    }
                    '3' => {
                        store.update_text(id, "| Name | Value |\n| --- | --- |\n| half |");
                        store.set_status(id, ChatMessageStatus::InProgress);
                        continue;
                    }
                    '4' => {
                        store.update_text(
                            id,
                            "| Name | Value |\n| --- | --- |\n| half | stable |\n",
                        );
                        store.set_status(id, ChatMessageStatus::Final);
                        continue;
                    }
                    '5' => {
                        store.update_text(id, "STREAM-DELTA-A");
                        store.set_status(id, ChatMessageStatus::InProgress);
                        continue;
                    }
                    '6' => {
                        store.append_delta(id, " + STREAM-DELTA-B");
                        store.set_status(id, ChatMessageStatus::Final);
                        continue;
                    }
                    _ => {}
                }
            }

            if let Some(id) = tool_message_id {
                match cmd {
                    '1' => {
                        store.append_tool_delta(id, "\nTOOL-OUTPUT-1");
                        continue;
                    }
                    '2' => {
                        store.append_tool_delta(id, "\nTOOL-OUTPUT-2");
                        continue;
                    }
                    '3' => {
                        store.set_tool_status(id, ChatToolCallStatus::Done);
                        continue;
                    }
                    '4' => {
                        store.set_tool_status(id, ChatToolCallStatus::Error);
                        continue;
                    }
                    _ => {}
                }
            }

            match cmd {
                'a' => {
                    store.push(ChatMessage::text(
                        store.next_message_id(),
                        ChatSender::Assistant,
                        "FOLLOW-1",
                    ));
                    continue;
                }
                'b' => {
                    store.push(ChatMessage::text(
                        store.next_message_id(),
                        ChatSender::Assistant,
                        "FOLLOW-2",
                    ));
                    continue;
                }
                'd' => {
                    store.push(ChatMessage::text(
                        store.next_message_id(),
                        ChatSender::Assistant,
                        "FOLLOW-3",
                    ));
                    continue;
                }
                'c' => {
                    input_handle.selection_binding().set(0);
                    input_handle.set_mode(ChatInputMode::Choice(ChatChoiceInputConfig::new(
                        "请选择一种回应方式",
                        vec!["简短回复".into(), "详细解释".into(), "给出示例".into()],
                    )));
                    continue;
                }
                'f' => {
                    input_handle.selection_binding().set(0);
                    input_handle.set_mode(ChatInputMode::Confirm(
                        ChatConfirmInputConfig::new("是否继续执行?")
                            .yes_label("继续")
                            .no_label("停止"),
                    ));
                    continue;
                }
                't' => {
                    input_handle.draft_binding().set(String::new());
                    input_handle.set_mode(ChatInputMode::text(
                        "Message",
                        Some("Type a message...".into()),
                    ));
                    continue;
                }
                _ => {}
            }
        }

        let screen: Rect = terminal.size()?.into();
        let _res = desktop.handle_event(&ev, screen);

        if let Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = ev
            && modifiers.contains(KeyModifiers::CONTROL)
        {
            break;
        }

        if actions.pop().is_some() {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn seed_messages(store: &ChatMessageStore, count: u64) {
    for idx in 0..count {
        let sender = if idx % 2 == 0 {
            ChatSender::User
        } else {
            ChatSender::Assistant
        };
        let message = ChatMessage::text(store.next_message_id(), sender, format!("MSG-{idx:02}"));
        store.push(message);
    }
}

fn submit_response_text(response: ChatInputResponse) -> String {
    match response {
        ChatInputResponse::Text(text) => format!("SUBMIT: text={text}"),
        ChatInputResponse::Choice { index, label } => {
            format!("SUBMIT: choice index={index} label={label}")
        }
        ChatInputResponse::Custom(text) => format!("SUBMIT: custom={text}"),
    }
}
