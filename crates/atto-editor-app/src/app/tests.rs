    use super::*;

    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crossterm::event::{
        Event, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("atto_editor_app_{prefix}_{nanos}"))
    }

    fn ctrl_alt_key(ch: char) -> Event {
        Event::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ))
    }

    fn ctrl_shift_key(ch: char) -> Event {
        Event::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ))
    }

    fn ctrl_key(ch: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL))
    }

    #[test]
    fn command_prefix_shows_which_key_and_exact_dispatches_action() {
        let screen = Rect::new(0, 0, 80, 24);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let mut desktop = Desktop::new(Theme::dark(), build_menu(actions.clone()));
        let mut keymap =
            commands::app_command_registry().key_sequence_engine(DEFAULT_KEY_SEQUENCE_TIMEOUT);
        let ignored = DesktopEventResult::ignored();

        handle_command_key_event(
            &mut desktop,
            &ctrl_alt_key('k'),
            screen,
            &ignored,
            &mut keymap,
            &state,
            &actions,
        )
        .expect("prefix handled");

        let popup = desktop.which_key().expect("which-key popup");
        assert_eq!(popup.prefix_label, commands::command_prefix().label());
        assert!(
            popup
                .choices
                .iter()
                .any(|choice| choice.command_id == "file.save" && choice.title == "Save")
        );

        handle_command_key_event(
            &mut desktop,
            &ctrl_alt_key('a'),
            screen,
            &ignored,
            &mut keymap,
            &state,
            &actions,
        )
        .expect("exact handled");

        assert!(desktop.which_key().is_none());
        assert_eq!(actions.drain(), vec![AppAction::Save]);
    }

    #[test]
    fn command_keymap_ignores_consumed_single_keys() {
        let screen = Rect::new(0, 0, 80, 24);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let mut desktop = Desktop::new(Theme::dark(), build_menu(actions.clone()));
        let mut keymap =
            commands::app_command_registry().key_sequence_engine(DEFAULT_KEY_SEQUENCE_TIMEOUT);
        let consumed = DesktopEventResult::consumed();

        handle_command_key_event(
            &mut desktop,
            &ctrl_alt_key('k'),
            screen,
            &consumed,
            &mut keymap,
            &state,
            &actions,
        )
        .expect("consumed key ignored");

        assert!(desktop.which_key().is_none());
        assert!(keymap.pending().is_empty());
        assert!(actions.is_empty());
    }

    #[test]
    fn command_palette_shortcut_dispatches_open_action() {
        let screen = Rect::new(0, 0, 80, 24);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let mut desktop = Desktop::new(Theme::dark(), build_menu(actions.clone()));
        let mut keymap =
            commands::app_command_registry().key_sequence_engine(DEFAULT_KEY_SEQUENCE_TIMEOUT);

        handle_command_key_event(
            &mut desktop,
            &ctrl_shift_key('p'),
            screen,
            &DesktopEventResult::ignored(),
            &mut keymap,
            &state,
            &actions,
        )
        .expect("command palette shortcut handled");

        assert_eq!(actions.drain(), vec![AppAction::OpenCommandPalette]);
    }

    #[test]
    fn file_picker_shortcut_dispatches_open_action() {
        let screen = Rect::new(0, 0, 80, 24);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let mut desktop = Desktop::new(Theme::dark(), build_menu(actions.clone()));
        let mut keymap =
            commands::app_command_registry().key_sequence_engine(DEFAULT_KEY_SEQUENCE_TIMEOUT);

        handle_command_key_event(
            &mut desktop,
            &ctrl_key('p'),
            screen,
            &DesktopEventResult::ignored(),
            &mut keymap,
            &state,
            &actions,
        )
        .expect("file picker shortcut handled");

        assert_eq!(actions.drain(), vec![AppAction::OpenFilePicker]);
    }

    #[test]
    fn global_search_shortcut_dispatches_open_action() {
        let screen = Rect::new(0, 0, 80, 24);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let mut desktop = Desktop::new(Theme::dark(), build_menu(actions.clone()));
        let mut keymap =
            commands::app_command_registry().key_sequence_engine(DEFAULT_KEY_SEQUENCE_TIMEOUT);

        handle_command_key_event(
            &mut desktop,
            &ctrl_shift_key('f'),
            screen,
            &DesktopEventResult::ignored(),
            &mut keymap,
            &state,
            &actions,
        )
        .expect("global search shortcut handled");

        assert_eq!(actions.drain(), vec![AppAction::OpenGlobalSearch]);
    }

    #[test]
    fn command_palette_items_come_from_command_registry() {
        let items = command_palette_items();

        let save = items
            .iter()
            .find(|item| item.title == "Save")
            .expect("save command item");
        assert_eq!(save.subtitle, "File");
        assert_eq!(save.shortcut.as_deref(), Some("Ctrl+Alt+K Ctrl+Alt+A"));
        assert!(items.iter().any(|item| item.title == "File Picker"));
        assert!(items.iter().any(|item| item.title == "Command Palette"));
        assert!(items.iter().any(|item| item.title == "Buffer Picker"));
        assert!(items.iter().any(|item| item.title == "Document Symbols"));
        assert!(items.iter().any(|item| item.title == "Workspace Symbols"));
        assert!(items.iter().any(|item| item.title == "Global Search"));
    }

    #[test]
    fn command_palette_close_restores_prior_focus() {
        let screen = Rect::new(0, 0, 80, 24);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let mut desktop = Desktop::new(Theme::dark(), build_menu(actions.clone()));
        let editor_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Editor",
                Rect::new(4, 4, 40, 10),
                Box::new(TextFn::new(|| "editor".to_string())),
            ),
            screen,
        );
        assert_eq!(desktop.wm.focused(), Some(editor_id));

        open_command_palette(&mut desktop, screen, &state);
        let palette_id = state
            .lock()
            .command_palette_window
            .expect("command palette window");
        assert_eq!(desktop.wm.focused(), Some(palette_id));

        assert!(desktop.close_window(palette_id));
        process_command_palette_events(&mut desktop, &state, &actions);

        assert_eq!(desktop.wm.focused(), Some(editor_id));
    }

    #[test]
    fn command_prefix_escape_clears_pending_and_which_key() {
        let screen = Rect::new(0, 0, 80, 24);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let mut desktop = Desktop::new(Theme::dark(), build_menu(actions.clone()));
        let mut keymap =
            commands::app_command_registry().key_sequence_engine(DEFAULT_KEY_SEQUENCE_TIMEOUT);
        let ignored = DesktopEventResult::ignored();

        handle_command_key_event(
            &mut desktop,
            &ctrl_alt_key('k'),
            screen,
            &ignored,
            &mut keymap,
            &state,
            &actions,
        )
        .expect("prefix handled");
        assert!(!keymap.pending().is_empty());
        assert!(desktop.which_key().is_some());

        handle_command_key_event(
            &mut desktop,
            &Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            screen,
            &DesktopEventResult::consumed(),
            &mut keymap,
            &state,
            &actions,
        )
        .expect("escape handled");

        assert!(keymap.pending().is_empty());
        assert!(desktop.which_key().is_none());
        assert!(actions.is_empty());
    }

    #[test]
    fn opening_file_inside_workspace_does_not_add_new_root() {
        let root = unique_temp_dir("workspace_open");
        fs::create_dir_all(root.join("src")).expect("create temp dirs");
        let file = root.join("src").join("main.rs");
        fs::write(&file, "fn main() {}\n").expect("write temp file");

        let screen = Rect::new(0, 0, 80, 24);
        let menu = MenuBar::new(Vec::new());
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        {
            let mut s = state.lock();
            s.workspace_roots = vec![canonicalize_best_effort(&root)];
        }

        let actions: EventQueue<AppAction> = EventQueue::new();
        let editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet> =
            atto_ui_editor::EditorThemeSet::default().into();
        let clipboard: atto_ui::reactive::Binding<String> = String::new().into();

        let commands = EventQueue::<EditorWindowCommand>::new();
        let view = EditorWindowView::new(
            actions.clone(),
            commands.clone(),
            editor_theme.clone(),
            clipboard.clone(),
            atto_ui_editor::DiagnosticsSummary::default().into(),
        );
        let editor_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Atto Editor",
                Rect::new(0, 0, 60, 16),
                Box::new(view),
            )
            .with_tag("atto-editor-app"),
            screen,
        );
        {
            let mut s = state.lock();
            s.editor_windows.insert(editor_id, commands);
            s.last_focused_editor = Some(editor_id);
        }

        open_path(
            &mut desktop,
            screen,
            &state,
            actions,
            editor_theme,
            clipboard,
            OpenTarget::NewTab,
            file,
        );

        let s = state.lock();
        assert_eq!(s.workspace_roots.len(), 1);
        assert!(s.explorer_commands.is_empty());
    }

    #[test]
    fn file_picker_items_rebuild_when_workspace_roots_change() {
        let root_a = unique_temp_dir("file_picker_root_a");
        let root_b = unique_temp_dir("file_picker_root_b");
        fs::create_dir_all(root_a.join("src")).expect("create root a");
        fs::create_dir_all(root_b.join("src")).expect("create root b");
        fs::write(root_a.join("src").join("main.rs"), "fn main() {}\n").expect("write main");
        fs::write(root_b.join("src").join("lib.rs"), "pub fn lib() {}\n").expect("write lib");

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        state.lock().workspace_roots = vec![root_a.clone()];

        let first = file_picker_items(&state);
        assert!(first.iter().any(|item| item.title == "src/main.rs"));
        assert!(!first.iter().any(|item| item.title == "src/lib.rs"));

        state.lock().workspace_roots = vec![root_b.clone()];
        let second = file_picker_items(&state);

        assert!(second.iter().any(|item| item.title == "src/lib.rs"));
        assert!(!second.iter().any(|item| item.title == "src/main.rs"));
        assert_eq!(
            state
                .lock()
                .file_picker_cache
                .as_ref()
                .map(|cache| cache.roots.clone()),
            Some(vec![canonicalize_best_effort(&root_b)])
        );
    }

    #[test]
    fn document_symbol_items_jump_to_selection_offset() {
        let outline = DocumentOutline::new(vec![DocumentSymbol {
            name: "main".to_string(),
            detail: Some("fn()".to_string()),
            kind: SymbolKind::Function,
            range: editor_core::SymbolRange::new(0, 10),
            selection_range: editor_core::SymbolRange::new(3, 7),
            children: Vec::new(),
            data_json: None,
        }]);

        let items = document_symbol_items(&outline);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "main");
        assert_eq!(
            items[0].action,
            AppAction::JumpTo(JumpTarget::CharOffset { offset: 3 })
        );
    }

    #[test]
    fn workspace_symbol_items_reject_non_file_uris_explicitly() {
        let symbols = vec![WorkspaceSymbol {
            name: "remote".to_string(),
            detail: None,
            kind: SymbolKind::Function,
            location: editor_core::SymbolLocation {
                uri: "untitled://remote".to_string(),
                range: editor_core::Utf16Range::new(
                    editor_core::Utf16Position::new(2, 4),
                    editor_core::Utf16Position::new(2, 10),
                ),
            },
            container_name: None,
            data_json: None,
        }];

        let items = workspace_symbol_items(&symbols);

        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0].action,
            AppAction::ShowStatusMessage(message)
                if message.contains("Unsupported workspace symbol URI")
        ));
    }

    #[test]
    fn global_search_items_open_file_and_jump_to_match() {
        let root = PathBuf::from("/tmp/workspace");
        let result = GlobalSearchResult {
            path: root.join("src/main.rs"),
            line: 4,
            column: 8,
            text: "let todo = \"TODO\";".to_string(),
            ranges: Vec::new(),
        };

        let items = global_search_items(std::slice::from_ref(&root), std::slice::from_ref(&result));

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "src/main.rs:5:9");
        assert_eq!(
            items[0].action,
            AppAction::OpenPathAndJump {
                path: result.path,
                target: JumpTarget::CharPosition { line: 4, column: 8 },
            }
        );
    }

    #[test]
    fn buffer_picker_selects_tab_by_stable_id_after_close() -> anyhow::Result<()> {
        let root = unique_temp_dir("buffer_picker_tabs");
        fs::create_dir_all(&root)?;
        let first = root.join("one.rs");
        let second = root.join("two.rs");
        let third = root.join("three.rs");
        fs::write(&first, "// one\n")?;
        fs::write(&second, "// two\n")?;
        fs::write(&third, "// three\n")?;

        let screen = Rect::new(0, 0, 90, 28);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(Vec::new()));
        let editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet> =
            atto_ui_editor::EditorThemeSet::default().into();
        let clipboard: atto_ui::reactive::Binding<String> = String::new().into();
        let commands = EventQueue::<EditorWindowCommand>::new();
        let editor_events = EventQueue::<atto_ui_editor::EditorEvent>::new();
        let diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary> =
            atto_ui_editor::DiagnosticsSummary::default().into();
        let editor_status: Binding<EditorStatus> = EditorStatus::default().into();
        let tab_summaries: Binding<Vec<EditorTabSummary>> = Vec::new().into();
        let editor = EditorWindowView::new_with_bindings(
            actions.clone(),
            commands.clone(),
            editor_events,
            editor_theme.clone(),
            clipboard.clone(),
            EditorWindowBindings::new(
                diagnostics_summary.clone(),
                editor_status.clone(),
                tab_summaries.clone(),
            ),
        );
        let editor_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Atto Editor",
                default_editor_rect(Desktop::layout(screen).work_area, 0),
                Box::new(editor),
            ),
            screen,
        );
        {
            let mut s = state.lock();
            s.editor_windows.insert(editor_id, commands.clone());
            s.editor_diagnostics.insert(editor_id, diagnostics_summary);
            s.editor_statuses.insert(editor_id, editor_status);
            s.editor_tab_summaries
                .insert(editor_id, tab_summaries.clone());
            s.last_focused_editor = Some(editor_id);
        }

        commands.push(EditorWindowCommand::OpenFile(first.clone()));
        commands.push(EditorWindowCommand::OpenFile(second));
        commands.push(EditorWindowCommand::OpenFile(third));
        let backend = TestBackend::new(screen.width, screen.height);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| desktop.draw(f))?;

        let summaries = tab_summaries.get();
        assert_eq!(summaries.len(), 3);
        let first_tab_id = summaries[0].tab_id;
        let second_tab_id = summaries[1].tab_id;

        commands.push(EditorWindowCommand::CloseActiveTab);
        terminal.draw(|f| desktop.draw(f))?;
        let summaries_after_close = tab_summaries.get();
        assert_eq!(summaries_after_close.len(), 2);
        assert_eq!(summaries_after_close[0].tab_id, first_tab_id);
        assert_eq!(summaries_after_close[1].tab_id, second_tab_id);

        let item = buffer_picker_items(&desktop, &state)
            .into_iter()
            .find(|item| item.title == "one.rs")
            .expect("first tab picker item");
        assert_eq!(
            item.action,
            AppAction::SelectEditorTab {
                window: editor_id,
                tab_id: first_tab_id,
            }
        );

        handle_action(
            &mut desktop,
            screen,
            &state,
            &actions,
            item.action,
            &Property::new(None),
            &Property::new(None),
            &Property::new(String::new()),
            editor_theme,
            clipboard,
        )?;
        terminal.draw(|f| desktop.draw(f))?;

        let active = tab_summaries
            .get()
            .into_iter()
            .find(|summary| summary.active)
            .expect("active tab summary");
        assert_eq!(active.tab_id, first_tab_id);
        assert_eq!(active.path, Some(canonicalize_best_effort(&first)));

        Ok(())
    }

    #[test]
    fn workspace_edit_marks_open_tab_dirty() -> anyhow::Result<()> {
        let root = unique_temp_dir("workspace_edit_dirty");
        fs::create_dir_all(&root)?;
        let file = root.join("main.rs");
        fs::write(&file, "hello world\n")?;
        let file = canonicalize_best_effort(&file);

        let screen = Rect::new(0, 0, 90, 28);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(Vec::new()));
        let editor_theme: Binding<atto_ui_editor::EditorThemeSet> =
            atto_ui_editor::EditorThemeSet::default().into();
        let clipboard: Binding<String> = String::new().into();
        let commands = EventQueue::<EditorWindowCommand>::new();
        let editor_events = EventQueue::<atto_ui_editor::EditorEvent>::new();
        let diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary> =
            atto_ui_editor::DiagnosticsSummary::default().into();
        let editor_status: Binding<EditorStatus> = EditorStatus::default().into();
        let tab_summaries: Binding<Vec<EditorTabSummary>> = Vec::new().into();
        let workspace_state = WorkspaceState::shared();
        workspace_state
            .lock()
            .set_workspace_roots(vec![canonicalize_best_effort(&root)]);
        let editor = EditorWindowView::new_with_workspace_bindings(
            actions,
            commands.clone(),
            editor_events,
            editor_theme,
            clipboard,
            workspace_state.clone(),
            EditorWindowBindings::new(diagnostics_summary, editor_status, tab_summaries.clone()),
        );
        desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Atto Editor",
                default_editor_rect(Desktop::layout(screen).work_area, 0),
                Box::new(editor),
            ),
            screen,
        );

        commands.push(EditorWindowCommand::OpenFile(file.clone()));
        let backend = TestBackend::new(screen.width, screen.height);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| desktop.draw(f))?;
        assert_eq!(tab_summaries.get().len(), 1);
        assert!(!tab_summaries.get()[0].dirty);

        let mut changes = serde_json::Map::new();
        changes.insert(
            editor_core_lsp::path_to_file_uri(&file),
            serde_json::json!([
                {
                    "range": {
                        "start": { "line": 0, "character": 6 },
                        "end": { "line": 0, "character": 11 }
                    },
                    "newText": "atto"
                }
            ]),
        );
        workspace_state
            .lock()
            .apply_workspace_edit(&serde_json::json!({ "changes": changes }))
            .map_err(anyhow::Error::msg)?;

        terminal.draw(|f| desktop.draw(f))?;
        let summaries = tab_summaries.get();
        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].dirty);
        assert!(workspace_state.lock().take_last_error().is_none());

        Ok(())
    }

    #[test]
    fn explorer_window_uses_wm_dock_and_editor_clamps_to_reserved_area() {
        let screen = Rect::new(0, 0, 90, 28);
        let work = Desktop::layout(screen).work_area;
        let menu = MenuBar::new(Vec::new());
        let mut desktop = Desktop::new(Theme::dark(), menu);
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let actions: EventQueue<AppAction> = EventQueue::new();

        let explorer = ExplorerWindowView::new(
            actions.clone(),
            state.lock().explorer_commands.clone(),
            Vec::new(),
        );
        let explorer_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Explorer",
                Rect::default(),
                Box::new(explorer),
            )
            .with_dock(Some(explorer_dock_config(
                DockSide::Left,
                DEFAULT_EXPLORER_DOCK_SIZE,
            ))),
            screen,
        );
        state.lock().explorer_window = Some(explorer_id);

        let editor_commands = EventQueue::<EditorWindowCommand>::new();
        let editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet> =
            atto_ui_editor::EditorThemeSet::default().into();
        let clipboard: atto_ui::reactive::Binding<String> = String::new().into();
        let editor = EditorWindowView::new(
            actions,
            editor_commands,
            editor_theme,
            clipboard,
            atto_ui_editor::DiagnosticsSummary::default().into(),
        );
        let editor_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Atto Editor",
                default_editor_rect(work, 0),
                Box::new(editor),
            ),
            screen,
        );

        assert_eq!(
            desktop.wm.window(explorer_id).expect("explorer").rect.get(),
            Rect::new(0, 1, DEFAULT_EXPLORER_DOCK_SIZE, 26)
        );

        let editor_rect = desktop.wm.window(editor_id).expect("editor").rect.get();
        assert!(
            editor_rect.x >= DEFAULT_EXPLORER_DOCK_SIZE,
            "editor should be clamped into WM effective work area, got {editor_rect:?}"
        );
    }

    #[test]
    fn dock_explorer_window_updates_dock_side_and_preserves_size() {
        let screen = Rect::new(0, 0, 90, 28);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let menu = MenuBar::new(Vec::new());
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let explorer = ExplorerWindowView::new(
            actions.clone(),
            state.lock().explorer_commands.clone(),
            Vec::new(),
        );
        let explorer_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Explorer",
                Rect::default(),
                Box::new(explorer),
            )
            .with_dock(Some(explorer_dock_config(DockSide::Left, 41))),
            screen,
        );
        {
            let mut s = state.lock();
            s.explorer_window = Some(explorer_id);
            s.explorer_size = Some(41);
        }

        dock_explorer_window(&mut desktop, screen, &state, DockSide::Right);

        let dock = desktop
            .wm
            .window(explorer_id)
            .expect("explorer")
            .dock
            .get()
            .expect("dock config");
        assert_eq!(dock.side, DockSide::Right);
        assert_eq!(dock.size, 41);
        assert_eq!(state.lock().explorer_dock, DockSide::Right);
        assert_eq!(state.lock().explorer_size, Some(41));

        let backend = TestBackend::new(screen.width, screen.height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| desktop.draw(f)).expect("draw");

        assert_eq!(
            desktop.wm.window(explorer_id).expect("explorer").rect.get(),
            Rect::new(49, 1, 41, 26)
        );
    }

    #[test]
    fn sync_explorer_dock_state_records_resized_dock_size() {
        let screen = Rect::new(0, 0, 90, 28);
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let actions: EventQueue<AppAction> = EventQueue::new();
        let menu = MenuBar::new(Vec::new());
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let explorer =
            ExplorerWindowView::new(actions, state.lock().explorer_commands.clone(), Vec::new());
        let explorer_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Explorer",
                Rect::default(),
                Box::new(explorer),
            )
            .with_dock(Some(explorer_dock_config(
                DockSide::Left,
                DEFAULT_EXPLORER_DOCK_SIZE,
            ))),
            screen,
        );
        state.lock().explorer_window = Some(explorer_id);

        let mut dock = desktop
            .wm
            .window(explorer_id)
            .expect("explorer")
            .dock
            .get()
            .expect("dock config");
        dock.size = 43;
        dock.side = DockSide::Right;
        desktop
            .wm
            .window_mut(explorer_id)
            .expect("explorer")
            .dock
            .set(Some(dock));

        sync_explorer_dock_state(&desktop, &state);

        let s = state.lock();
        assert_eq!(s.explorer_dock, DockSide::Right);
        assert_eq!(s.explorer_size, Some(43));
    }

    #[test]
    fn toggle_explorer_window_preserves_dock_side_and_size_on_reopen() {
        let screen = Rect::new(0, 0, 90, 28);
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let actions: EventQueue<AppAction> = EventQueue::new();
        let menu = MenuBar::new(Vec::new());
        let mut desktop = Desktop::new(Theme::dark(), menu);

        {
            let mut s = state.lock();
            s.explorer_dock = DockSide::Right;
            s.explorer_size = Some(41);
        }
        toggle_explorer_window(&mut desktop, screen, &state, actions.clone());
        let first_id = state.lock().explorer_window.expect("explorer reopened");

        let dock = desktop
            .wm
            .window(first_id)
            .expect("explorer")
            .dock
            .get()
            .expect("dock config");
        assert_eq!(dock.side, DockSide::Right);
        assert_eq!(dock.size, 41);

        toggle_explorer_window(&mut desktop, screen, &state, actions.clone());
        assert!(state.lock().explorer_window.is_none());

        toggle_explorer_window(&mut desktop, screen, &state, actions);
        let reopened_id = state.lock().explorer_window.expect("explorer reopened");
        assert_ne!(first_id, reopened_id);
        let reopened_dock = desktop
            .wm
            .window(reopened_id)
            .expect("explorer")
            .dock
            .get()
            .expect("dock config");
        assert_eq!(reopened_dock.side, DockSide::Right);
        assert_eq!(reopened_dock.size, 41);
    }

    #[test]
    fn active_editor_commands_falls_back_when_explorer_is_focused() {
        let screen = Rect::new(0, 0, 90, 28);
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let actions: EventQueue<AppAction> = EventQueue::new();
        let menu = MenuBar::new(Vec::new());
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let explorer = ExplorerWindowView::new(
            actions.clone(),
            state.lock().explorer_commands.clone(),
            Vec::new(),
        );
        let explorer_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Explorer",
                Rect::default(),
                Box::new(explorer),
            )
            .with_dock(Some(explorer_dock_config(
                DockSide::Left,
                DEFAULT_EXPLORER_DOCK_SIZE,
            ))),
            screen,
        );

        let editor_commands = EventQueue::<EditorWindowCommand>::new();
        let editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet> =
            atto_ui_editor::EditorThemeSet::default().into();
        let clipboard: atto_ui::reactive::Binding<String> = String::new().into();
        let editor = EditorWindowView::new(
            actions,
            editor_commands.clone(),
            editor_theme,
            clipboard,
            atto_ui_editor::DiagnosticsSummary::default().into(),
        );
        let editor_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Atto Editor",
                default_editor_rect(Desktop::layout(screen).work_area, 0),
                Box::new(editor),
            ),
            screen,
        );
        {
            let mut s = state.lock();
            s.explorer_window = Some(explorer_id);
            s.editor_windows.insert(editor_id, editor_commands.clone());
            s.last_focused_editor = Some(editor_id);
        }

        desktop.wm.focus(explorer_id);
        assert_eq!(desktop.wm.focused(), Some(explorer_id));

        active_editor_commands(&desktop, &state)
            .expect("last focused editor commands")
            .push(EditorWindowCommand::SaveActive);

        let drained = editor_commands.drain();
        assert!(matches!(
            drained.as_slice(),
            [EditorWindowCommand::SaveActive]
        ));
    }

    #[test]
    fn diagnostics_statusbar_uses_last_focused_editor_when_explorer_is_focused() {
        let screen = Rect::new(0, 0, 90, 28);
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let actions: EventQueue<AppAction> = EventQueue::new();
        let menu = MenuBar::new(Vec::new());
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let explorer = ExplorerWindowView::new(
            actions.clone(),
            state.lock().explorer_commands.clone(),
            Vec::new(),
        );
        let explorer_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Explorer",
                Rect::default(),
                Box::new(explorer),
            )
            .with_dock(Some(explorer_dock_config(
                DockSide::Left,
                DEFAULT_EXPLORER_DOCK_SIZE,
            ))),
            screen,
        );

        let editor_commands = EventQueue::<EditorWindowCommand>::new();
        let editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet> =
            atto_ui_editor::EditorThemeSet::default().into();
        let clipboard: atto_ui::reactive::Binding<String> = String::new().into();
        let diagnostics_summary: atto_ui::reactive::Binding<atto_ui_editor::DiagnosticsSummary> =
            atto_ui_editor::DiagnosticsSummary {
                errors: 1,
                warnings: 2,
                infos: 3,
                hints: 4,
            }
            .into();
        let editor_status: atto_ui::reactive::Binding<EditorStatus> = EditorStatus {
            path: Some(PathBuf::from("src/main.rs")),
            language: "rust".to_string(),
            dirty: false,
        }
        .into();
        let editor = EditorWindowView::new_with_status(
            actions,
            editor_commands.clone(),
            EventQueue::<atto_ui_editor::EditorEvent>::new(),
            editor_theme,
            clipboard,
            diagnostics_summary.clone(),
            editor_status.clone(),
        );
        let editor_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Atto Editor",
                default_editor_rect(Desktop::layout(screen).work_area, 0),
                Box::new(editor),
            ),
            screen,
        );
        {
            let mut s = state.lock();
            s.explorer_window = Some(explorer_id);
            s.editor_windows.insert(editor_id, editor_commands);
            s.editor_diagnostics.insert(editor_id, diagnostics_summary);
            s.editor_statuses.insert(editor_id, editor_status);
            s.last_focused_editor = Some(editor_id);
        }

        desktop.wm.focus(explorer_id);
        update_diagnostics_statusbar(&mut desktop, &state);

        let backend = TestBackend::new(90, 1);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|f| {
                desktop
                    .status
                    .draw(f, Rect::new(0, 0, 90, 1), &desktop.theme)
            })
            .expect("draw statusbar");

        let mut row = String::new();
        let buf = terminal.backend().buffer();
        for x in 0..90 {
            row.push_str(buf[(x, 0)].symbol());
        }
        assert!(
            row.contains("E:1 W:2"),
            "expected diagnostics summary in statusbar, got {row:?}"
        );
        assert!(
            row.contains("rust"),
            "expected language in statusbar, got {row:?}"
        );
    }

    #[test]
    fn explorer_double_click_opens_file_in_editor() -> anyhow::Result<()> {
        let root = unique_temp_dir("explorer_double_click_opens");
        fs::create_dir_all(&root)?;
        let file_path = root.join("open_me.txt");
        fs::write(&file_path, "HELLO_FROM_EDITOR\n")?;

        let screen = Rect::new(0, 0, 90, 28);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet> =
            atto_ui_editor::EditorThemeSet::default().into();
        let clipboard: atto_ui::reactive::Binding<String> = String::new().into();

        let open_file_result: Property<Option<PathBuf>> = Property::new(None);
        let save_as_result: Property<Option<PathBuf>> = Property::new(None);
        let open_folder_input: Property<String> = Property::new(String::new());

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        state.lock().workspace_roots = vec![canonicalize_best_effort(&root)];

        let menu = build_menu(actions.clone());
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let work = Desktop::layout(screen).work_area;

        // Explorer window.
        let explorer_commands = state.lock().explorer_commands.clone();
        let explorer_view = ExplorerWindowView::new(
            actions.clone(),
            explorer_commands.clone(),
            state.lock().workspace_roots.clone(),
        );
        let explorer_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Explorer",
                Rect::default(),
                Box::new(explorer_view),
            )
            .with_dock(Some(explorer_dock_config(
                DockSide::Left,
                DEFAULT_EXPLORER_DOCK_SIZE,
            ))),
            screen,
        );
        state.lock().explorer_window = Some(explorer_id);

        // Editor window.
        let editor_rect = default_editor_rect(work, 0);
        let editor_commands: EventQueue<EditorWindowCommand> = EventQueue::new();
        let editor_view = EditorWindowView::new(
            actions.clone(),
            editor_commands.clone(),
            editor_theme.clone(),
            clipboard.clone(),
            atto_ui_editor::DiagnosticsSummary::default().into(),
        );
        let editor_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Atto Editor",
                editor_rect,
                Box::new(editor_view),
            ),
            screen,
        );
        {
            let mut s = state.lock();
            s.editor_windows.insert(editor_id, editor_commands.clone());
            s.last_focused_editor = Some(editor_id);
        }

        // Draw once so scrollable children have a `last_area` for mouse routing.
        let backend = TestBackend::new(screen.width, screen.height);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| desktop.draw(f))?;

        let inner = desktop
            .wm
            .window(explorer_id)
            .expect("explorer window")
            .inner_rect();
        // The explorer file tree is borderless, so content starts at the inner top:
        // row 0 is the workspace root directory, row 1 is the first child file.
        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: inner.x + 4,
            row: inner.y + 1,
            modifiers: KeyModifiers::NONE,
        });

        // First click selects; second click should trigger open action.
        let _ = desktop.handle_event(&click, screen);
        let _ = desktop.handle_event(&click, screen);

        // Process queued actions (like the main tick loop does).
        for action in actions.drain() {
            handle_action(
                &mut desktop,
                screen,
                &state,
                &actions,
                action,
                &open_file_result,
                &save_as_result,
                &open_folder_input,
                editor_theme.clone(),
                clipboard.clone(),
            )?;
        }

        terminal.draw(|f| desktop.draw(f))?;

        let mut snapshot = String::new();
        let buf = terminal.backend().buffer();
        for y in 0..screen.height {
            for x in 0..screen.width {
                snapshot.push_str(buf[(x, y)].symbol());
            }
            snapshot.push('\n');
        }

        assert!(
            snapshot.contains("HELLO_FROM_EDITOR"),
            "expected editor to render file contents; got:\n{snapshot}"
        );

        Ok(())
    }
