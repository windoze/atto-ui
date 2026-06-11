use std::collections::{BTreeMap, HashMap};
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
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

use atto_ui::ComponentValue;
use atto_ui_chat::{
    Artifact, ArtifactBlock, ArtifactId, ArtifactKind, ArtifactViewer, AttachmentBlock, ChatBlock,
    ChatBlockId, ChatChoiceInputConfig, ChatConfirmInputConfig, ChatInputHandle, ChatInputMode,
    ChatInputResponse, ChatMessage, ChatMessageId, ChatMessageList, ChatMessageStore, ChatPanel,
    ChatRole, ChatTurnStatus, DiffBlock, DiffData, EditDecision, NoticeBlock, NoticeLevel,
    TextArtifactViewer, TextBlock, ThinkingBlock, TodoBlock, TodoItem, TodoState, ToolInput,
    ToolOutput, ToolResultBlock, ToolStatus, ToolUseBlock,
};

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        event::EnableMouseCapture,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES),
        cursor::Show
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let actions: EventQueue<()> = EventQueue::new();
    let args: Vec<String> = std::env::args().collect();
    let streaming_markdown = args.iter().any(|arg| arg == "--streaming-markdown");
    let tool_call = args.iter().any(|arg| arg == "--tool-call");
    let artifact_link = args.iter().any(|arg| arg == "--artifact-link");
    let block_mapping = args.iter().any(|arg| arg == "--block-mapping");
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
    let artifacts = if artifact_link {
        seed_artifacts(&store)
    } else {
        HashMap::new()
    };
    let tool_block_ids = if block_mapping {
        seed_block_mapping_messages(&store);
        None
    } else if tool_call {
        let id = store.next_message_id();
        let message = ChatMessage::tool_call(id, "build", ToolStatus::Running, "TOOL-START");
        let tool_use_id = message
            .blocks
            .iter()
            .find_map(|block| match block {
                ChatBlock::ToolUse(_) => Some(block.id()),
                _ => None,
            })
            .expect("tool use block should exist");
        let tool_result_id = message
            .blocks
            .iter()
            .find_map(|block| match block {
                ChatBlock::ToolResult(_) => Some(block.id()),
                _ => None,
            })
            .expect("tool result block should exist");
        store.push(message);
        Some((tool_use_id, tool_result_id))
    } else {
        None
    };
    let streaming_block_ids = if block_mapping || tool_block_ids.is_some() || artifact_link {
        None
    } else if streaming_markdown {
        let id = store.next_message_id();
        let message = ChatMessage::text(id, ChatRole::Assistant, "STREAMING-MARKDOWN")
            .with_status(ChatTurnStatus::Streaming);
        let text_id = message.blocks[0].id();
        store.push(message);
        Some((id, text_id))
    } else {
        seed_messages(&store, 28);
        None
    };

    let input_handle = ChatInputHandle::new();
    let load_counter = Arc::new(AtomicU64::new(0));
    let open_artifacts: EventQueue<ArtifactId> = EventQueue::new();
    let list = ChatMessageList::new(store.clone())
        .wrap_width(56)
        .show_timestamps(false)
        .on_open_artifact({
            let open_artifacts = open_artifacts.clone();
            move |artifact_id| open_artifacts.push(artifact_id)
        })
        .on_load_more({
            let store = store.clone();
            let counter = load_counter.clone();
            move || {
                let page = counter.fetch_add(1, Ordering::Relaxed) + 1;
                let mut older = Vec::new();
                for idx in 0..3u64 {
                    let message = ChatMessage::text(
                        store.next_message_id(),
                        ChatRole::System,
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
                ChatRole::System,
                text,
            ));
        }
    });
    let panel = ChatPanel::new(list, input_panel);

    let mut desktop = Desktop::new(Theme::dark(), menu);
    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;
    let window_height = if block_mapping { 40 } else { 18 };

    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Chat Snapshot",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(2),
                width: 60.min(work.width.saturating_sub(2)).max(30),
                height: window_height.min(work.height.saturating_sub(2)).max(12),
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
            if let Some((id, text_id)) = streaming_block_ids {
                match cmd {
                    '1' => {
                        set_text_block(
                            &store,
                            id,
                            text_id,
                            "```rust\nfn main() {\n    println!(\"STREAMING-CODE\");",
                        );
                        continue;
                    }
                    '2' => {
                        store.append_text_delta(text_id, "\n}\n```");
                        store.set_turn_status(id, ChatTurnStatus::Complete);
                        continue;
                    }
                    '3' => {
                        set_text_block(
                            &store,
                            id,
                            text_id,
                            "| Name | Value |\n| --- | --- |\n| half |",
                        );
                        store.set_turn_status(id, ChatTurnStatus::Streaming);
                        continue;
                    }
                    '4' => {
                        set_text_block(
                            &store,
                            id,
                            text_id,
                            "| Name | Value |\n| --- | --- |\n| half | stable |\n",
                        );
                        store.set_turn_status(id, ChatTurnStatus::Complete);
                        continue;
                    }
                    '5' => {
                        set_text_block(&store, id, text_id, "STREAM-DELTA-A");
                        store.set_turn_status(id, ChatTurnStatus::Streaming);
                        continue;
                    }
                    '6' => {
                        store.append_text_delta(text_id, " + STREAM-DELTA-B");
                        store.set_turn_status(id, ChatTurnStatus::Complete);
                        continue;
                    }
                    _ => {}
                }
            }

            if let Some((tool_use_id, tool_result_id)) = tool_block_ids {
                match cmd {
                    '1' => {
                        store.append_tool_output(tool_result_id, "\nTOOL-OUTPUT-1");
                        continue;
                    }
                    '2' => {
                        store.append_tool_output(tool_result_id, "\nTOOL-OUTPUT-2");
                        continue;
                    }
                    '3' => {
                        store.set_tool_status(tool_use_id, ToolStatus::Done);
                        continue;
                    }
                    '4' => {
                        store.set_tool_status(tool_use_id, ToolStatus::Error);
                        continue;
                    }
                    _ => {}
                }
            }

            match cmd {
                'a' => {
                    store.push(ChatMessage::text(
                        store.next_message_id(),
                        ChatRole::Assistant,
                        "FOLLOW-1",
                    ));
                    continue;
                }
                'b' => {
                    store.push(ChatMessage::text(
                        store.next_message_id(),
                        ChatRole::Assistant,
                        "FOLLOW-2",
                    ));
                    continue;
                }
                'd' => {
                    store.push(ChatMessage::text(
                        store.next_message_id(),
                        ChatRole::Assistant,
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

        for artifact_id in open_artifacts.drain() {
            if let Some(artifact) = artifacts.get(&artifact_id).cloned() {
                let mut viewer = TextArtifactViewer::new(&mut desktop, screen);
                viewer.open(artifact);
            }
        }

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
        PopKeyboardEnhancementFlags,
        LeaveAlternateScreen,
        event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn seed_messages(store: &ChatMessageStore, count: u64) {
    for idx in 0..count {
        let sender = if idx % 2 == 0 {
            ChatRole::User
        } else {
            ChatRole::Assistant
        };
        if idx == 1 {
            let id = store.next_message_id();
            store.push(ChatMessage::new(
                id,
                sender,
                vec![
                    ChatBlock::Text(TextBlock {
                        id: snapshot_block_id(id, 0),
                        markdown: "MSG-01".to_string(),
                        streaming: false,
                    }),
                    ChatBlock::Thinking(ThinkingBlock {
                        id: snapshot_block_id(id, 1),
                        markdown: "MULTI-BLOCK-THINKING".to_string(),
                        streaming: false,
                        collapsed: false,
                    }),
                    ChatBlock::Notice(NoticeBlock {
                        id: snapshot_block_id(id, 2),
                        level: NoticeLevel::Info,
                        text: "MULTI-BLOCK-NOTICE".to_string(),
                    }),
                ],
            ));
            continue;
        }
        let message = ChatMessage::text(store.next_message_id(), sender, format!("MSG-{idx:02}"));
        store.push(message);
    }
}

fn seed_block_mapping_messages(store: &ChatMessageStore) {
    let id = store.next_message_id();
    let mut input = BTreeMap::new();
    input.insert(
        "path".to_string(),
        ComponentValue::String("src/lib.rs".to_string()),
    );
    input.insert("count".to_string(), ComponentValue::U64(2));

    store.push(ChatMessage::new(
        id,
        ChatRole::Assistant,
        vec![
            ChatBlock::Text(TextBlock {
                id: snapshot_block_id(id, 0),
                markdown: "BLOCK-TEXT".to_string(),
                streaming: false,
            }),
            ChatBlock::Thinking(ThinkingBlock {
                id: snapshot_block_id(id, 1),
                markdown: "BLOCK-THINKING".to_string(),
                streaming: false,
                collapsed: false,
            }),
            ChatBlock::ToolUse(ToolUseBlock {
                id: snapshot_block_id(id, 2),
                call_id: "call-json".to_string(),
                name: "json_tool".to_string(),
                input: ToolInput::Json(ComponentValue::Map(input)),
                status: ToolStatus::Pending,
                approval: None,
                collapsed: false,
            }),
            ChatBlock::ToolResult(ToolResultBlock {
                id: snapshot_block_id(id, 3),
                call_id: "call-ansi".to_string(),
                ok: true,
                exit_code: Some(0),
                output: ToolOutput::Ansi("\u{1b}[32mANSI-GREEN\u{1b}[0m\nANSI-PLAIN".to_string()),
                collapsed: false,
            }),
            ChatBlock::ToolResult(ToolResultBlock {
                id: snapshot_block_id(id, 4),
                call_id: "call-markdown".to_string(),
                ok: true,
                exit_code: None,
                output: ToolOutput::Markdown("MARKDOWN-OUTPUT".to_string()),
                collapsed: false,
            }),
            ChatBlock::ToolResult(ToolResultBlock {
                id: snapshot_block_id(id, 5),
                call_id: "call-diff".to_string(),
                ok: true,
                exit_code: None,
                output: ToolOutput::Diff(DiffData {
                    unified: "+TOOL-DIFF\n-TOOL-DIFF-OLD".to_string(),
                }),
                collapsed: false,
            }),
            ChatBlock::Diff(DiffBlock {
                id: snapshot_block_id(id, 6),
                path: "src/main.rs".to_string(),
                diff: DiffData {
                    unified: "+INLINE-DIFF".to_string(),
                },
                decision: EditDecision::Pending,
            }),
            ChatBlock::Todo(TodoBlock {
                id: snapshot_block_id(id, 7),
                items: vec![
                    TodoItem {
                        text: "BLOCK-TODO-PENDING".to_string(),
                        state: TodoState::Pending,
                    },
                    TodoItem {
                        text: "BLOCK-TODO-DONE".to_string(),
                        state: TodoState::Done,
                    },
                ],
            }),
            ChatBlock::Attachment(AttachmentBlock {
                id: snapshot_block_id(id, 8),
                name: "report.txt".to_string(),
                url: Some("file:///tmp/report.txt".to_string()),
                mime: Some("text/plain".to_string()),
            }),
            ChatBlock::Notice(NoticeBlock {
                id: snapshot_block_id(id, 9),
                level: NoticeLevel::Warning,
                text: "BLOCK-NOTICE".to_string(),
            }),
            ChatBlock::Artifact(ArtifactBlock {
                id: snapshot_block_id(id, 10),
                kind: ArtifactKind::Code,
                anchor: ArtifactId::new("block-artifact"),
                title: "block-artifact.rs".to_string(),
            }),
        ],
    ));
}

fn snapshot_block_id(message_id: ChatMessageId, ordinal: u64) -> ChatBlockId {
    ChatBlockId::new(
        message_id
            .0
            .saturating_mul(1_000)
            .saturating_add(ordinal + 1),
    )
}

fn set_text_block(
    store: &ChatMessageStore,
    message_id: ChatMessageId,
    block_id: ChatBlockId,
    markdown: &str,
) {
    let should_update = store
        .with_block(block_id, |block| match block {
            ChatBlock::Text(text) => text.markdown != markdown,
            _ => false,
        })
        .unwrap_or(false);
    if !should_update {
        return;
    }

    let markdown = markdown.to_string();
    store.update_message(message_id, |message| {
        if let Some(ChatBlock::Text(text)) = message
            .blocks
            .iter_mut()
            .find(|block| block.id() == block_id)
        {
            text.markdown = markdown;
        }
    });
}

fn seed_artifacts(store: &ChatMessageStore) -> HashMap<ArtifactId, Artifact> {
    let code_id = ArtifactId::new("code-main");
    let diff_id = ArtifactId::new("diff-main");

    store.push(ChatMessage::artifact(
        store.next_message_id(),
        ChatRole::Assistant,
        ArtifactKind::Code,
        code_id.clone(),
        "main.rs",
    ));
    store.push(ChatMessage::artifact(
        store.next_message_id(),
        ChatRole::Assistant,
        ArtifactKind::Diff,
        diff_id.clone(),
        "main.patch",
    ));

    let mut artifacts = HashMap::new();
    artifacts.insert(
        code_id.clone(),
        Artifact::new(
            code_id,
            ArtifactKind::Code,
            "main.rs",
            "fn main() {\n    println!(\"CODE-ARTIFACT\");\n}",
        ),
    );
    artifacts.insert(
        diff_id.clone(),
        Artifact::new(
            diff_id,
            ArtifactKind::Diff,
            "main.patch",
            "--- a/main.rs\n+++ b/main.rs\n@@ -1,3 +1,4 @@\n fn main() {\n-    println!(\"old\");\n+    println!(\"DIFF-ARTIFACT\");\n }",
        ),
    );
    artifacts
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
