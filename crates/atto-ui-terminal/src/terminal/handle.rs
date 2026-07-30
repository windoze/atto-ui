//! External accessor API: [`TerminalHandle`] (cloneable handle over the shared
//! state) and [`TerminalSnapshot`] (full text snapshot incl. scrollback).

use super::*;

#[derive(Clone)]
pub struct TerminalHandle {
    pub(crate) shared: Arc<Mutex<TerminalShared>>,
}

impl TerminalHandle {
    /// Feeds bytes into the terminal emulator (ANSI output stream).
    pub fn process_output(&self, bytes: &[u8]) {
        let (responses, dispatches) = {
            let mut shared = self.shared.lock();
            let decoded = shared.tmux_dcs_passthrough.decode(bytes);
            shared.parser.process(&decoded);
            let events = shared.parser.callbacks_mut().take_events();
            let responses = collect_dsr_responses(&mut shared, &decoded);
            let dispatches = shared.apply_callback_events(events);
            shared.prune_cleared_command_marks();
            (responses, dispatches)
        };
        for response in responses {
            forward_input(&self.shared, &response);
        }
        dispatch_terminal_callback_events(&self.shared, dispatches);
    }

    pub fn process_output_str(&self, text: &str) {
        self.process_output(text.as_bytes());
    }

    /// Sends a user input event to the terminal (encoded to bytes).
    pub fn send_event(&self, event: &Event) {
        let shared = self.shared.lock();
        let screen = shared.parser.screen();
        let bytes = match event {
            Event::Key(key) => encode_key_event(screen, *key),
            Event::Paste(text) => Some(encode_paste_text(screen, text)),
            Event::Mouse(m) => encode_mouse_event(screen, *m, None, MouseCoordinateSpace::Absolute),
            _ => None,
        };
        if let Some(bytes) = bytes {
            drop(shared);
            dispatch_input(&self.shared, &bytes);
        }
    }

    /// Pushes raw input bytes to the terminal input stream.
    pub fn send_input_bytes(&self, bytes: &[u8]) {
        dispatch_input(&self.shared, bytes);
    }

    /// Explicitly resizes the parser screen and attached PTY, if a subprocess is running.
    pub fn resize(&self, rows: u16, cols: u16) -> bool {
        resize_terminal(&self.shared, rows, cols)
    }

    /// Returns and clears the queued input bytes.
    pub fn take_input(&self) -> Vec<u8> {
        let mut shared = self.shared.lock();
        let mut out = Vec::with_capacity(shared.input.len());
        while let Some(b) = shared.input.pop_front() {
            out.push(b);
        }
        out
    }

    pub fn set_capture(&self, capture: bool) {
        self.shared.lock().set_capture(capture);
    }

    pub fn capture(&self) -> bool {
        self.shared.lock().capture
    }

    /// Applies a validated terminal configuration to this live terminal instance.
    pub fn apply_config(&self, config: &TerminalConfig) -> Result<()> {
        let runtime_config = TerminalRuntimeConfig::from_config(config)?;
        self.shared.lock().apply_runtime_config(runtime_config);
        Ok(())
    }

    /// Refreshes the ANSI palette from a theme's [`TerminalTheme`].
    ///
    /// Use this when the surrounding [`Theme`] changes at runtime so the
    /// emulator's colors track the active theme.
    pub fn apply_theme(&self, theme: &Theme) {
        self.shared.lock().palette = TerminalPalette::from_theme(theme);
    }

    pub fn set_scrollback_len(&self, len: usize) {
        self.shared.lock().set_scrollback_len(len);
    }

    pub fn scrollback_len(&self) -> usize {
        self.shared.lock().scrollback_len
    }

    pub fn set_release_shortcut(&self, shortcut: TerminalShortcut) {
        self.shared.lock().release_shortcut = shortcut;
    }

    pub fn release_shortcut(&self) -> TerminalShortcut {
        self.shared.lock().release_shortcut
    }

    /// Updates the spawn-time shell integration policy.
    pub fn set_shell_integration(&self, integration: TerminalShellIntegration) {
        self.shared.lock().shell_integration = integration;
    }

    /// Returns the current spawn-time shell integration policy.
    pub fn shell_integration(&self) -> TerminalShellIntegration {
        self.shared.lock().shell_integration
    }

    /// Updates tmux-compatible probe variables used by future subprocess spawns.
    pub fn set_tmux_environment(&self, config: TerminalTmuxEnvironmentConfig) {
        self.shared.lock().tmux_environment = config;
    }

    /// Returns the current tmux-compatible probe environment configuration.
    pub fn tmux_environment(&self) -> TerminalTmuxEnvironmentConfig {
        self.shared.lock().tmux_environment.clone()
    }

    /// Returns the last non-fatal shell integration injection error, if any.
    pub fn last_shell_integration_error(&self) -> Option<String> {
        self.shared.lock().last_shell_integration_error.clone()
    }

    /// Returns the latest cwd reported by OSC 7 shell integration, if any.
    pub fn current_cwd(&self) -> Option<String> {
        self.shared.lock().current_cwd.clone()
    }

    /// Returns the cursor shape most recently requested through DECSCUSR.
    pub fn cursor_shape(&self) -> TerminalCursorShape {
        self.shared.lock().cursor_shape
    }

    /// Updates the terminal prefix shortcut. Only plain `Ctrl+<ASCII letter>` is accepted.
    pub fn set_prefix_shortcut(&self, shortcut: TerminalShortcut) -> Result<()> {
        let shortcut = normalize_prefix_shortcut(shortcut)?;
        self.shared.lock().set_prefix_shortcut(shortcut);
        Ok(())
    }

    pub fn prefix_shortcut(&self) -> TerminalShortcut {
        self.shared.lock().prefix_shortcut
    }

    /// Adds or replaces one prefix command binding at runtime.
    pub fn set_prefix_binding(&self, shortcut: TerminalShortcut, command: TerminalPrefixCommand) {
        self.shared
            .lock()
            .set_prefix_binding(TerminalPrefixBinding::new(shortcut, command));
    }

    /// Replaces the full prefix command table at runtime.
    pub fn set_prefix_bindings(&self, bindings: impl IntoIterator<Item = TerminalPrefixBinding>) {
        self.shared.lock().set_prefix_bindings(bindings);
    }

    pub fn prefix_bindings(&self) -> Vec<TerminalPrefixBinding> {
        self.shared.lock().prefix_bindings.clone()
    }

    /// Returns whether copy-mode is currently active.
    pub fn copy_mode(&self) -> bool {
        self.shared.lock().copy_mode.is_some()
    }

    /// Returns the current copy-mode cursor position, if copy-mode is active.
    pub fn copy_mode_cursor(&self) -> Option<TerminalSelectionPosition> {
        self.shared
            .lock()
            .copy_mode
            .as_ref()
            .map(|mode| mode.cursor)
    }

    /// Returns the last text copied into the terminal-local copy buffer.
    pub fn copied_text(&self) -> Option<String> {
        self.shared.lock().copy_buffer.clone()
    }

    /// Copies the active selection into the terminal-local copy buffer.
    pub fn copy_selection(&self) -> Option<String> {
        let text = { self.shared.lock().copy_selection() };
        if let Some(text) = &text {
            dispatch_system_clipboard_copy(&self.shared, text);
        }
        text
    }

    /// Pastes the terminal-local copy buffer back into the subprocess input stream.
    pub fn paste_copied_text(&self) -> bool {
        let bytes = { self.shared.lock().paste_copy_buffer_bytes() };
        let Some(bytes) = bytes else {
            return false;
        };
        dispatch_input(&self.shared, &bytes);
        true
    }

    /// Starts a terminal text selection at an absolute scrollback/screen position.
    pub fn begin_selection(&self, position: TerminalSelectionPosition) {
        self.shared.lock().selection.start(position);
    }

    /// Extends the active terminal text selection to an absolute position.
    pub fn update_selection(&self, position: TerminalSelectionPosition) {
        self.shared.lock().selection.update(position);
    }

    /// Clears the active terminal text selection.
    pub fn clear_selection(&self) -> bool {
        self.shared.lock().selection.clear()
    }

    /// Returns the normalized active terminal text selection range.
    pub fn selection_range(&self) -> Option<TerminalSelectionRange> {
        self.shared.lock().selection.range()
    }

    /// Converts a visible terminal cell into the absolute coordinate used by selections.
    pub fn selection_position_for_view_cell(
        &self,
        row: u16,
        col: u16,
    ) -> TerminalSelectionPosition {
        let mut shared = self.shared.lock();
        let max_scrollback = shared.max_scrollback();
        let screen = shared.parser.screen();
        let (rows, cols) = screen.size();
        position_for_view_cell(max_scrollback, screen.scrollback(), rows, cols, row, col)
    }

    /// Returns text currently covered by the active selection.
    pub fn selected_text(&self) -> Option<String> {
        let mut shared = self.shared.lock();
        shared.selected_text()
    }

    /// Returns the latest OSC 0/2 window title, if one has been observed.
    pub fn window_title(&self) -> Option<String> {
        self.shared.lock().window_title.clone()
    }

    /// Returns the latest OSC 0/1 window icon name, if one has been observed.
    pub fn window_icon_name(&self) -> Option<String> {
        self.shared.lock().window_icon_name.clone()
    }

    /// Returns the number of audible bell requests observed in terminal output.
    pub fn audible_bell_count(&self) -> u64 {
        self.shared.lock().audible_bell_count
    }

    /// Returns the latest OSC 52 clipboard-copy request, if one has been observed.
    pub fn last_clipboard_copy(&self) -> Option<TerminalClipboardCopy> {
        self.shared.lock().last_clipboard_copy.clone()
    }

    /// Returns the last text sent to the configured system clipboard backend.
    pub fn last_system_clipboard_text(&self) -> Option<String> {
        self.shared.lock().last_system_clipboard_text.clone()
    }

    /// Returns the last system clipboard sync error, if the most recent sync failed.
    pub fn last_system_clipboard_error(&self) -> Option<String> {
        self.shared.lock().last_system_clipboard_error.clone()
    }

    /// Returns OSC 133/7 command blocks observed in terminal output.
    pub fn command_blocks(&self) -> Vec<TerminalCommandBlock> {
        self.shared.lock().command_marks.clone()
    }

    /// Returns the command block index covering an absolute terminal position.
    pub fn command_block_index_at_position(
        &self,
        position: TerminalSelectionPosition,
    ) -> Option<usize> {
        self.shared.lock().command_block_index_at_position(position)
    }

    /// Scrolls so the previous command block is visible at the top of the viewport.
    pub fn scroll_to_previous_command_block(&self) -> Option<usize> {
        self.shared.lock().scroll_to_previous_command_block()
    }

    /// Scrolls so the next command block is visible at the top of the viewport.
    pub fn scroll_to_next_command_block(&self) -> Option<usize> {
        self.shared.lock().scroll_to_next_command_block()
    }

    /// Selects the complete output range for a command block.
    pub fn select_command_block_output(&self, index: usize) -> Option<TerminalSelectionRange> {
        self.shared.lock().select_command_block_output(index)
    }

    /// Copies the command text for a command block into the terminal-local copy buffer.
    pub fn copy_command_block_command(&self, index: usize) -> Option<String> {
        let text = {
            self.shared
                .lock()
                .copy_command_block_text(index, CommandBlockTextKind::Command)
        };
        if let Some(text) = &text {
            dispatch_system_clipboard_copy(&self.shared, text);
        }
        text
    }

    /// Copies the output text for a command block into the terminal-local copy buffer.
    pub fn copy_command_block_output(&self, index: usize) -> Option<String> {
        let text = {
            self.shared
                .lock()
                .copy_command_block_text(index, CommandBlockTextKind::Output)
        };
        if let Some(text) = &text {
            dispatch_system_clipboard_copy(&self.shared, text);
        }
        text
    }

    /// Sends a command block's command text back to the subprocess as a new command.
    pub fn rerun_command_block(&self, index: usize) -> bool {
        let bytes = { self.shared.lock().command_block_rerun_bytes(index) };
        let Some(bytes) = bytes else {
            return false;
        };
        dispatch_input(&self.shared, &bytes);
        true
    }

    /// Returns the exit code for the most recently completed command block, if reported.
    pub fn last_exit_code(&self) -> Option<i32> {
        self.shared
            .lock()
            .command_marks
            .iter()
            .rev()
            .find(|block| block.end.is_some())
            .and_then(|block| block.exit_code)
    }

    /// Returns whether a subprocess is currently attached and has not reported exit.
    pub fn is_running(&self) -> bool {
        self.shared.lock().process_running
    }

    /// Returns the last recorded subprocess exit status, if the process has exited.
    pub fn exit_status(&self) -> Option<ExitStatus> {
        self.shared.lock().exit_status.clone()
    }

    /// Snapshot of the terminal contents including scrollback.
    pub fn snapshot(&self) -> TerminalSnapshot {
        let mut shared = self.shared.lock();
        let max_scrollback = shared.max_scrollback();
        let (rows, cols, current_offset) = {
            let screen = shared.parser.screen();
            let (rows, cols) = screen.size();
            let current_offset = screen.scrollback();
            (rows, cols, current_offset)
        };
        let screen = shared.parser.screen_mut();

        let mut lines = Vec::with_capacity(max_scrollback + rows as usize);
        let mut start = 0;
        while start < max_scrollback {
            let offset = max_scrollback - start;
            screen.set_scrollback(offset);
            let chunk: Vec<String> = screen.rows(0, cols).collect();
            let take = (max_scrollback - start).min(rows as usize);
            lines.extend(chunk.into_iter().take(take));
            start += take;
        }

        screen.set_scrollback(0);
        lines.extend(screen.rows(0, cols));

        screen.set_scrollback(current_offset);

        TerminalSnapshot {
            lines,
            cols,
            rows,
            scrollback: max_scrollback,
        }
    }
}

/// Full text snapshot of the terminal contents including scrollback.
#[derive(Clone, Debug)]
pub struct TerminalSnapshot {
    pub lines: Vec<String>,
    pub cols: u16,
    pub rows: u16,
    pub scrollback: usize,
}

impl TerminalSnapshot {
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}
