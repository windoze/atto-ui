//! [`TerminalEmulator`]: the terminal widget itself — builder/spawn API,
//! mouse / alt-screen wheel routing, and the `Component` / trait impls.

use super::*;

pub struct TerminalEmulator {
    pub(crate) shared: Arc<Mutex<TerminalShared>>,
    pub(crate) last_area: Option<Rect>,
    pub(crate) capture_on_click: bool,
    pub(crate) command_block_presentation: TerminalCommandBlockPresentation,
    pub(crate) process: Option<TerminalProcess>,
    pub(crate) on_close: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl TerminalEmulator {
    pub fn new() -> Self {
        let runtime_config = TerminalRuntimeConfig::from_config(&TerminalConfig::default())
            .expect("default terminal config must be valid");
        let parser = terminal_parser(24, 80, runtime_config.scrollback_len);
        let shared = TerminalShared {
            parser,
            scrollback_len: runtime_config.scrollback_len,
            palette: runtime_config.palette,
            alternate_screen_scroll: runtime_config.alternate_screen_scroll,
            input: VecDeque::new(),
            on_input: None,
            input_forward: None,
            on_exit: None,
            on_window_title: None,
            on_window_icon_name: None,
            on_audible_bell: None,
            on_clipboard_copy: None,
            on_command_finished: None,
            system_clipboard: Some(Arc::new(DefaultTerminalSystemClipboard)),
            pty_resize: None,
            shell_integration: runtime_config.shell_integration,
            tmux_environment: runtime_config.tmux_environment,
            last_shell_integration_error: None,
            exit_status: None,
            process_running: false,
            window_title: None,
            window_icon_name: None,
            audible_bell_count: 0,
            last_clipboard_copy: None,
            last_system_clipboard_text: None,
            last_system_clipboard_error: None,
            capture: true,
            capture_suspended_by_blur: false,
            release_shortcut: runtime_config.release_shortcut,
            prefix_shortcut: runtime_config.prefix_shortcut,
            prefix_bindings: default_prefix_bindings(),
            prefix_pending: false,
            copy_mode: None,
            copy_buffer: None,
            selection: TerminalSelectionState::default(),
            command_marks: Vec::new(),
            current_cwd: None,
            cursor_shape: runtime_config.cursor_shape,
            dsr_tail: Vec::with_capacity(4),
            tmux_dcs_passthrough: TmuxDcsPassthroughDecoder::default(),
        };

        Self {
            shared: Arc::new(Mutex::new(shared)),
            last_area: None,
            capture_on_click: true,
            command_block_presentation: TerminalCommandBlockPresentation::default(),
            process: None,
            on_close: None,
        }
    }

    pub fn from_config(config: &TerminalConfig) -> Result<Self> {
        Self::new().config(config)
    }

    pub fn handle(&self) -> TerminalHandle {
        TerminalHandle {
            shared: Arc::clone(&self.shared),
        }
    }

    /// Explicitly resizes the parser screen and attached PTY, if a subprocess is running.
    pub fn resize(&mut self, rows: u16, cols: u16) -> bool {
        resize_terminal(&self.shared, rows, cols)
    }

    pub fn scrollback_len(self, len: usize) -> Self {
        self.shared.lock().set_scrollback_len(len);
        self
    }

    pub fn config(self, config: &TerminalConfig) -> Result<Self> {
        self.handle().apply_config(config)?;
        Ok(self)
    }

    pub fn capture(self, capture: bool) -> Self {
        self.shared.lock().set_capture(capture);
        self
    }

    pub fn release_shortcut(self, shortcut: TerminalShortcut) -> Self {
        self.shared.lock().release_shortcut = shortcut;
        self
    }

    /// Sets the terminal prefix shortcut. Only plain `Ctrl+<ASCII letter>` is accepted.
    pub fn prefix_shortcut(self, shortcut: TerminalShortcut) -> Result<Self> {
        let shortcut = normalize_prefix_shortcut(shortcut)?;
        self.shared.lock().set_prefix_shortcut(shortcut);
        Ok(self)
    }

    /// Sets the terminal prefix key letter, using `Ctrl+letter` as the actual shortcut.
    pub fn prefix_key(self, letter: char) -> Result<Self> {
        self.prefix_shortcut(prefix_shortcut_from_letter(letter)?)
    }

    /// Adds or replaces one prefix command binding.
    pub fn prefix_binding(
        self,
        shortcut: TerminalShortcut,
        command: TerminalPrefixCommand,
    ) -> Self {
        self.shared
            .lock()
            .set_prefix_binding(TerminalPrefixBinding::new(shortcut, command));
        self
    }

    /// Replaces the prefix command table.
    pub fn prefix_bindings(
        self,
        bindings: impl IntoIterator<Item = TerminalPrefixBinding>,
    ) -> Self {
        self.shared.lock().set_prefix_bindings(bindings);
        self
    }

    pub fn scroll_step(self, step: u16) -> Self {
        self.shared.lock().alternate_screen_scroll.step = step.max(1);
        self
    }

    pub fn capture_on_click(mut self, enabled: bool) -> Self {
        self.capture_on_click = enabled;
        self
    }

    /// Enables or disables visual presentation for OSC 133 command blocks.
    pub fn command_block_presentation(
        mut self,
        presentation: TerminalCommandBlockPresentation,
    ) -> Self {
        self.command_block_presentation = presentation;
        self
    }

    /// Replaces the system clipboard backend used by selection, copy-mode, and OSC 52 copies.
    pub fn system_clipboard<C>(self, clipboard: C) -> Self
    where
        C: TerminalSystemClipboard + 'static,
    {
        self.shared.lock().system_clipboard = Some(Arc::new(clipboard));
        self
    }

    /// Disables system clipboard writes while preserving the terminal-local copy buffer.
    pub fn without_system_clipboard(self) -> Self {
        self.shared.lock().system_clipboard = None;
        self
    }

    /// Configures whether supported interactive shell spawns receive OSC 133/7 integration.
    pub fn shell_integration(self, integration: TerminalShellIntegration) -> Self {
        self.shared.lock().shell_integration = integration;
        self
    }

    /// Configures tmux-compatible probe variables for future subprocess spawns.
    pub fn tmux_environment(self, config: TerminalTmuxEnvironmentConfig) -> Self {
        self.shared.lock().tmux_environment = config;
        self
    }

    pub fn on_input<F>(self, callback: F) -> Self
    where
        F: Fn(&[u8]) + Send + Sync + 'static,
    {
        self.shared.lock().on_input = Some(Arc::new(callback));
        self
    }

    pub fn on_close<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_close = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires once when the attached subprocess exits.
    pub fn on_exit<F>(self, callback: F) -> Self
    where
        F: Fn(ExitStatus) + Send + Sync + 'static,
    {
        self.shared.lock().on_exit = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires when OSC 0/2 updates the window title.
    pub fn on_window_title<F>(self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.shared.lock().on_window_title = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires when OSC 0/1 updates the window icon name.
    pub fn on_window_icon_name<F>(self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.shared.lock().on_window_icon_name = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires when BEL requests an audible bell.
    pub fn on_audible_bell<F>(self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.shared.lock().on_audible_bell = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires when OSC 52 requests a clipboard copy.
    pub fn on_clipboard_copy<F>(self, callback: F) -> Self
    where
        F: Fn(&TerminalClipboardCopy) + Send + Sync + 'static,
    {
        self.shared.lock().on_clipboard_copy = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires when OSC 133 marks a command as finished.
    pub fn on_command_finished<F>(self, callback: F) -> Self
    where
        F: Fn(&TerminalCommandBlock) + Send + Sync + 'static,
    {
        self.shared.lock().on_command_finished = Some(Arc::new(callback));
        self
    }

    /// Spawns a subprocess attached to the terminal's PTY.
    pub fn spawn_process(&mut self, command: &str, args: &[String]) -> Result<()> {
        let mut cmd = CommandBuilder::new(command);
        for arg in args {
            cmd.arg(arg);
        }
        self.spawn_command(cmd)
    }

    /// Spawns a subprocess from a reusable terminal session spec.
    pub fn spawn_session(&mut self, session: &TerminalSessionSpec) -> Result<()> {
        self.spawn_command(session.command_builder())
    }

    /// Spawns a subprocess using a custom command builder.
    pub fn spawn_command(&mut self, mut cmd: CommandBuilder) -> Result<()> {
        let (shell_integration, tmux_environment) = {
            let shared = self.shared.lock();
            (shared.shell_integration, shared.tmux_environment.clone())
        };
        prepare_spawn_command(&mut cmd, &tmux_environment)?;
        let shell_integration_files = match prepare_shell_integration(&mut cmd, shell_integration) {
            Ok(files) => {
                self.shared.lock().last_shell_integration_error = None;
                files
            }
            Err(error) => {
                self.shared.lock().last_shell_integration_error = Some(error.to_string());
                None
            }
        };

        self.stop_process();
        {
            let mut shared = self.shared.lock();
            shared.exit_status = None;
            shared.process_running = false;
            shared.pty_resize = None;
        }

        let (rows, cols) = {
            let shared = self.shared.lock();
            shared.parser.screen().size()
        };

        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let child = pair.slave.spawn_command(cmd)?;
        let child = Arc::new(Mutex::new(child));
        let master = pair.master;
        let writer = master.take_writer()?;
        let reader = master.try_clone_reader()?;
        let pty_resize = TerminalPtyResize::new(master, rows, cols);
        {
            let mut shared = self.shared.lock();
            shared.process_running = true;
            shared.pty_resize = Some(pty_resize.clone());
        }

        let handle = self.handle();
        let shared_for_reader = Arc::clone(&self.shared);
        let child_for_reader = Arc::clone(&child);
        let reader_alive = Arc::new(AtomicBool::new(true));
        let reader_alive_thread = Arc::clone(&reader_alive);
        let reader_thread = thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 8192];
            while reader_alive_thread.load(Ordering::Relaxed) {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => handle.process_output(&buf[..n]),
                    Err(_) => break,
                }
            }
            if reader_alive_thread.load(Ordering::Relaxed) {
                try_record_child_exit(&shared_for_reader, &child_for_reader);
            }
        });
        let shared_for_watcher = Arc::clone(&self.shared);
        let child_for_watcher = Arc::clone(&child);
        let exit_watcher_alive = Arc::clone(&reader_alive);
        let exit_watcher_thread = thread::spawn(move || {
            while exit_watcher_alive.load(Ordering::Relaxed) {
                if try_record_child_exit(&shared_for_watcher, &child_for_watcher) {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        });

        let forward_writer = Arc::new(Mutex::new(writer));
        self.shared.lock().input_forward = Some(Arc::new(move |bytes| {
            if bytes.is_empty() {
                return;
            }
            let mut writer = forward_writer.lock();
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }));

        self.process = Some(TerminalProcess {
            _pty_resize: pty_resize,
            child,
            reader_alive,
            reader_thread: Some(reader_thread),
            exit_watcher_thread: Some(exit_watcher_thread),
            _shell_integration_files: shell_integration_files,
        });

        Ok(())
    }

    /// Stops the currently attached subprocess, if any.
    pub fn stop_process(&mut self) {
        {
            let mut shared = self.shared.lock();
            shared.input_forward = None;
            shared.process_running = false;
            shared.pty_resize = None;
        }
        if let Some(mut process) = self.process.take() {
            process.shutdown(&self.shared);
        }
    }

    pub(crate) fn handle_scrollback_wheel(&mut self, event: MouseEvent) -> bool {
        let mut shared = self.shared.lock();
        // Work in `usize` directly: `step` is a `u16` whose full range would
        // overflow `i16` (and negating `i16::MIN` panics in debug builds).
        let step = usize::from(shared.alternate_screen_scroll.step);
        let max = shared.max_scrollback();
        let current = shared.parser.screen().scrollback();
        let desired = match event.kind {
            MouseEventKind::ScrollUp => current.saturating_add(step).min(max),
            MouseEventKind::ScrollDown => current.saturating_sub(step),
            _ => return false,
        };
        if desired != current {
            shared.parser.screen_mut().set_scrollback(desired);
            return true;
        }
        false
    }

    pub(crate) fn handle_alternate_screen_wheel(&mut self, event: MouseEvent) -> bool {
        let shared = self.shared.lock();
        let config = shared.alternate_screen_scroll;
        if !config.enabled {
            return false;
        }
        let shortcut = match event.kind {
            MouseEventKind::ScrollUp => config.scroll_up_key,
            MouseEventKind::ScrollDown => config.scroll_down_key,
            _ => return false,
        };
        let screen = shared.parser.screen();
        if !matches!(screen.mouse_protocol_mode(), vt100::MouseProtocolMode::None)
            || !screen.alternate_screen()
        {
            return false;
        }

        let Some(key_bytes) =
            encode_key_event(screen, KeyEvent::new(shortcut.code, shortcut.modifiers))
        else {
            return true;
        };
        let mut bytes = Vec::with_capacity(key_bytes.len() * usize::from(config.step));
        for _ in 0..config.step {
            bytes.extend_from_slice(&key_bytes);
        }
        drop(shared);
        dispatch_input(&self.shared, &bytes);
        true
    }

    pub(crate) fn handle_scrollback_key(&mut self, event: KeyEvent) -> bool {
        if event.kind == KeyEventKind::Release {
            return false;
        }
        let mut shared = self.shared.lock();
        if handle_command_navigation_key(&mut shared, event) {
            return true;
        }
        let max = shared.max_scrollback();
        let current = shared.parser.screen().scrollback();
        let rows = shared.parser.screen().size().0 as usize;
        let desired = match event.code {
            KeyCode::PageUp => current.saturating_add(rows).min(max),
            KeyCode::PageDown => current.saturating_sub(rows),
            KeyCode::Home => max,
            KeyCode::End => 0,
            _ => return false,
        };
        if desired != current {
            shared.parser.screen_mut().set_scrollback(desired);
            return true;
        }
        false
    }

    pub(crate) fn handle_local_mouse_selection(
        &mut self,
        event: MouseEvent,
        coordinate_space: MouseCoordinateSpace,
    ) -> bool {
        if !matches!(
            event.kind,
            MouseEventKind::Down(MouseButton::Left)
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left)
        ) {
            return false;
        }

        let mut shared = self.shared.lock();
        let mouse_reporting_enabled = !matches!(
            shared.parser.screen().mouse_protocol_mode(),
            vt100::MouseProtocolMode::None
        );
        let selection_requested = !mouse_reporting_enabled
            || event.modifiers.contains(KeyModifiers::SHIFT)
            || shared.selection.is_dragging();
        if !selection_requested {
            return false;
        }

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(position) = mouse_selection_position(
                    &mut shared,
                    self.last_area,
                    event,
                    coordinate_space,
                    false,
                ) else {
                    return false;
                };
                shared.selection.start(position);
                true
            }
            MouseEventKind::Drag(MouseButton::Left) if shared.selection.is_dragging() => {
                let Some(position) = mouse_selection_position(
                    &mut shared,
                    self.last_area,
                    event,
                    coordinate_space,
                    true,
                ) else {
                    return false;
                };
                shared.selection.update(position);
                true
            }
            MouseEventKind::Up(MouseButton::Left) if shared.selection.is_dragging() => {
                let include_cell = shared.selection.range().is_some();
                let Some(position) = mouse_selection_position(
                    &mut shared,
                    self.last_area,
                    event,
                    coordinate_space,
                    include_cell,
                ) else {
                    return false;
                };
                shared.selection.finish(position);
                let text = shared.copy_selection();
                drop(shared);
                if let Some(text) = text {
                    dispatch_system_clipboard_copy(&self.shared, &text);
                }
                true
            }
            _ => false,
        }
    }

    pub(crate) fn handle_copy_mode_mouse(&mut self, event: MouseEvent) -> bool {
        let mut shared = self.shared.lock();
        if shared.copy_mode.is_none() {
            return false;
        }
        let step = shared.alternate_screen_scroll.step as isize;
        match event.kind {
            MouseEventKind::ScrollUp => {
                let _ = shared.scroll_copy_mode_view(step);
            }
            MouseEventKind::ScrollDown => {
                let _ = shared.scroll_copy_mode_view(-step);
            }
            _ => {}
        }
        true
    }
}

impl Default for TerminalEmulator {
    fn default() -> Self {
        Self::new()
    }
}

impl ::atto_ui::composable::Component for TerminalEmulator {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        if area.width == 0 || area.height == 0 {
            return;
        }
        if let Some(process) = &mut self.process {
            process.record_exit_if_ready(&self.shared);
        }

        let (rows, cols) = (area.height, area.width);
        self.resize(rows, cols);

        let mut shared = self.shared.lock();
        if !ctx.is_focused {
            // Losing focus (e.g. a modal popup opened) auto-releases keyboard
            // capture. Remember that this was a transient, focus-driven release
            // so it can be restored when focus returns — unlike an intentional
            // release via the release shortcut.
            if shared.capture {
                shared.set_capture(false);
                shared.capture_suspended_by_blur = true;
            }
        } else if shared.capture_suspended_by_blur {
            // Focus came back after a focus-driven release: restore capture so
            // the keyboard keeps working without requiring a click.
            shared.set_capture(true);
            shared.capture_suspended_by_blur = false;
        }
        let selection_range = shared.selection.range();
        let copy_mode_cursor = shared.copy_mode.as_ref().map(|mode| mode.cursor);
        let command_blocks = if self.command_block_presentation.is_enabled()
            && !shared.parser.screen().alternate_screen()
        {
            shared.command_marks.clone()
        } else {
            // Command-block decorations (separators, output shading, failure
            // markers) belong to the primary scrollback. Full-screen apps on the
            // alternate screen manage their own layout, so suppress them there.
            Vec::new()
        };
        let max_scrollback = shared.max_scrollback();
        let cursor_shape = shared.cursor_shape;
        let cursor_color = ctx.theme.terminal.cursor;
        let cursor_text_color = ctx.theme.terminal.cursor_text;
        let palette = shared.palette.clone();
        let screen = shared.parser.screen_mut();
        let visible_top = visible_top_row(max_scrollback, screen.scrollback());

        // Default cell colors: an explicitly configured palette fg/bg wins;
        // otherwise fall back to the theme's terminal colors (not window_bg),
        // so a terminal that derives its palette from the theme stays coherent.
        let base_style = ctx.theme.window_bg;
        let base_fg = palette
            .foreground
            .or(Some(ctx.theme.terminal.foreground))
            .or(base_style.fg);
        let base_bg = palette
            .background
            .or(Some(ctx.theme.terminal.background))
            .or(base_style.bg);
        let command_output_style = command_output_style(ctx.theme);
        let command_separator_style = command_separator_style(ctx.theme);
        let command_failure_style = command_failure_style(ctx.theme);
        let selection_style = Style::default()
            .bg(ctx.theme.terminal.selection_bg)
            .fg(ctx.theme.terminal.selection_fg);

        let buf = frame.buffer_mut();
        for y in 0..area.height {
            let absolute_row = visible_top.saturating_add(usize::from(y));
            let row_presentation = command_row_presentation(&command_blocks, absolute_row);
            let separator_start = if row_presentation.separator {
                command_separator_start(screen, y, area.width)
            } else {
                area.width
            };
            let selected_ranges = selection_range
                .map(|range| {
                    selected_cell_ranges_for_screen_row(screen, y, absolute_row, area.width, range)
                })
                .unwrap_or_default();
            for x in 0..area.width {
                let cell = screen.cell(y, x);
                let mut is_wide_cont = cell.is_some_and(vt100::Cell::is_wide_continuation);
                let mut symbol = cell
                    .map(|c| {
                        if c.is_wide_continuation() || c.contents().is_empty() {
                            " "
                        } else {
                            c.contents()
                        }
                    })
                    .unwrap_or(" ");

                let style = cell
                    .map(|c| cell_style(c, base_fg, base_bg, &palette))
                    .unwrap_or(base_style);
                let style = if row_presentation.output {
                    style.patch(command_output_style)
                } else {
                    style
                };
                let style = if row_presentation.separator && x >= separator_start && !is_wide_cont {
                    symbol = COMMAND_SEPARATOR_SYMBOL;
                    style.patch(command_separator_style)
                } else {
                    style
                };
                let style = if row_presentation.failed_marker && x == area.width.saturating_sub(1) {
                    symbol = COMMAND_FAILURE_SYMBOL;
                    is_wide_cont = false;
                    style.patch(command_failure_style)
                } else {
                    style
                };
                let style = if selected_ranges
                    .iter()
                    .any(|(start, end)| x >= *start && x < *end)
                {
                    selection_style
                } else {
                    style
                };
                let style = if copy_mode_cursor.is_some_and(|cursor| {
                    absolute_row == cursor.row && x == cursor.col.min(area.width.saturating_sub(1))
                }) {
                    style.add_modifier(Modifier::REVERSED)
                } else {
                    style
                };

                let dst_x = area.x.saturating_add(x);
                let dst_y = area.y.saturating_add(y);
                if let Some(dst) = buf.cell_mut((dst_x, dst_y)) {
                    dst.set_symbol(symbol);
                    dst.set_style(style);
                    dst.set_skip(is_wide_cont);
                }
            }
        }

        if !screen.hide_cursor() && screen.scrollback() == 0 {
            let (cur_row, cur_col) = screen.cursor_position();
            if cur_row < area.height && cur_col < area.width {
                let dst_x = area.x.saturating_add(cur_col);
                let dst_y = area.y.saturating_add(cur_row);
                if let Some(dst) = buf.cell_mut((dst_x, dst_y)) {
                    apply_cursor_shape(dst, cursor_shape, cursor_color, cursor_text_color);
                }
            }
        }
    }
}

impl ::atto_ui::composable::DragAndDrop for TerminalEmulator {}

impl ::atto_ui::composable::Layout for TerminalEmulator {
    fn min_width(&self) -> u16 {
        1
    }

    fn min_height(&self) -> u16 {
        1
    }
}

impl ::atto_ui::composable::Scrollable for TerminalEmulator {
    fn is_scrollable(&self) -> bool {
        let mut shared = self.shared.lock();
        let max = shared.max_scrollback();
        max > 0
    }

    fn content_size(&self) -> (u16, u16) {
        let mut shared = self.shared.lock();
        let (rows, cols) = shared.parser.screen().size();
        let max = shared.max_scrollback().min(u16::MAX as usize);
        let height = rows.saturating_add(max as u16);
        (cols, height)
    }

    fn viewport_size(&self) -> (u16, u16) {
        let shared = self.shared.lock();
        let (rows, cols) = shared.parser.screen().size();
        (cols, rows)
    }

    fn scroll_config(&self) -> ScrollConfig {
        ScrollConfig::default()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        let mut shared = self.shared.lock();
        let max = shared.max_scrollback().min(u16::MAX as usize);
        let offset = shared.scrollback_offset().min(max);
        let y = max.saturating_sub(offset) as u16;
        (0, y)
    }

    fn set_scroll_offset(&mut self, _x: u16, y: u16) {
        let mut shared = self.shared.lock();
        shared.set_scrollback_from_scroll_offset(y);
    }
}

impl ::atto_ui::composable::FocusNav for TerminalEmulator {
    fn is_focusable(&self) -> bool {
        true
    }
}

impl ::atto_ui::composable::DynamicTree for TerminalEmulator {}

impl ::atto_ui::composable::EventHandling for TerminalEmulator {
    fn handle_event_capture(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        let Event::Key(key) = event else {
            return EventResult::ignored();
        };
        let mut shared = self.shared.lock();
        if !shared.capture {
            return EventResult::ignored();
        }
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => match handle_captured_key(&mut shared, *key) {
                CapturedKeyAction::Consumed => EventResult::consumed(),
                CapturedKeyAction::Component(action) => EventResult {
                    outcome: EventOutcome::Consumed,
                    action,
                    capture: Capture::None,
                },
                CapturedKeyAction::Dispatch(bytes) => {
                    drop(shared);
                    dispatch_input(&self.shared, &bytes);
                    EventResult::consumed()
                }
                CapturedKeyAction::SystemClipboardCopy(text) => {
                    drop(shared);
                    dispatch_system_clipboard_copy(&self.shared, &text);
                    EventResult::consumed()
                }
            },
            _ => EventResult::ignored(),
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        match event {
            Event::Key(key) => {
                let mut shared = self.shared.lock();
                if shared.capture {
                    match handle_captured_key(&mut shared, *key) {
                        CapturedKeyAction::Consumed => return EventResult::consumed(),
                        CapturedKeyAction::Component(action) => {
                            return EventResult {
                                outcome: EventOutcome::Consumed,
                                action,
                                capture: Capture::None,
                            };
                        }
                        CapturedKeyAction::Dispatch(bytes) => {
                            drop(shared);
                            dispatch_input(&self.shared, &bytes);
                            return EventResult::consumed();
                        }
                        CapturedKeyAction::SystemClipboardCopy(text) => {
                            drop(shared);
                            dispatch_system_clipboard_copy(&self.shared, &text);
                            return EventResult::consumed();
                        }
                    }
                }
                drop(shared);
                if self.handle_scrollback_key(*key) {
                    return EventResult::consumed();
                }
                EventResult::ignored()
            }
            Event::Paste(text) => {
                let shared = self.shared.lock();
                if !shared.capture {
                    return EventResult::ignored();
                }
                let bytes = encode_paste_text(shared.parser.screen(), text);
                drop(shared);
                dispatch_input(&self.shared, &bytes);
                EventResult::consumed()
            }
            Event::Mouse(m) => {
                let inside =
                    mouse_coords_local(self.last_area, *m, ctx.mouse_coordinate_space).is_some();
                if !inside {
                    return EventResult::ignored();
                }

                let mut shared = self.shared.lock();
                if !shared.capture {
                    if matches!(m.kind, MouseEventKind::Down(_)) && self.capture_on_click {
                        shared.set_capture(true);
                    } else {
                        drop(shared);
                        if self.handle_scrollback_wheel(*m) {
                            return EventResult::consumed();
                        }
                        return EventResult::ignored();
                    }
                }
                drop(shared);

                if self.handle_copy_mode_mouse(*m) {
                    return EventResult::consumed();
                }

                if self.handle_local_mouse_selection(*m, ctx.mouse_coordinate_space) {
                    return EventResult::consumed();
                }

                let shared = self.shared.lock();
                let screen = shared.parser.screen();
                if let Some(bytes) =
                    encode_mouse_event(screen, *m, self.last_area, ctx.mouse_coordinate_space)
                {
                    drop(shared);
                    dispatch_input(&self.shared, &bytes);
                    return EventResult::consumed();
                }
                drop(shared);
                if self.handle_alternate_screen_wheel(*m) {
                    return EventResult::consumed();
                }
                if self.handle_scrollback_wheel(*m) {
                    return EventResult::consumed();
                }
                EventResult::consumed()
            }
            _ => EventResult::ignored(),
        }
    }
}

impl Drop for TerminalEmulator {
    fn drop(&mut self) {
        self.stop_process();
        if let Some(cb) = self.on_close.take() {
            cb();
        }
    }
}
