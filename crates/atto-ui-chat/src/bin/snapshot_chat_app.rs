use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
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
    ChatChoiceInputConfig, ChatConfirmInputConfig, ChatInputHandle, ChatInputMode, ChatMessage,
    ChatMessageList, ChatMessageStore, ChatPanel, ChatSender,
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
    seed_messages(&store, 28);

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
    let input_panel = input_handle.panel();
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
        {
            if modifiers.is_empty() {
                match cmd {
                    'c' => {
                        input_handle.set_mode(ChatInputMode::Choice(ChatChoiceInputConfig::new(
                            "请选择一种回应方式",
                            vec!["简短回复".into(), "详细解释".into(), "给出示例".into()],
                        )));
                        continue;
                    }
                    'f' => {
                        input_handle.set_mode(ChatInputMode::Confirm(
                            ChatConfirmInputConfig::new("是否继续执行?")
                                .yes_label("继续")
                                .no_label("停止"),
                        ));
                        continue;
                    }
                    't' => {
                        input_handle.set_mode(ChatInputMode::text(
                            "Message",
                            Some("Type a message...".into()),
                        ));
                        continue;
                    }
                    _ => {}
                }
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
        let message = ChatMessage::text(
            store.next_message_id(),
            sender,
            format!("MSG-{idx:02}"),
        );
        store.push(message);
    }
}
