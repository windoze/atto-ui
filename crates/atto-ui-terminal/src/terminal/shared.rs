//! [`TerminalShared`]: the shared mutable terminal state (parser, scrollback,
//! copy-mode, selection, command-blocks, prefix config) and its methods.

use super::*;

pub(crate) struct TerminalShared {
    pub(crate) parser: TerminalParser,
    pub(crate) scrollback_len: usize,
    pub(crate) palette: TerminalPalette,
    pub(crate) alternate_screen_scroll: TerminalAlternateScreenScroll,
    pub(crate) input: VecDeque<u8>,
    pub(crate) on_input: Option<InputCallback>,
    pub(crate) input_forward: Option<InputCallback>,
    pub(crate) on_exit: Option<ExitCallback>,
    pub(crate) on_window_title: Option<TextCallback>,
    pub(crate) on_window_icon_name: Option<TextCallback>,
    pub(crate) on_audible_bell: Option<BellCallback>,
    pub(crate) on_clipboard_copy: Option<ClipboardCopyCallback>,
    pub(crate) on_command_finished: Option<CommandFinishedCallback>,
    pub(crate) system_clipboard: Option<SystemClipboard>,
    pub(crate) pty_resize: Option<TerminalPtyResize>,
    pub(crate) shell_integration: TerminalShellIntegration,
    pub(crate) tmux_environment: TerminalTmuxEnvironmentConfig,
    pub(crate) last_shell_integration_error: Option<String>,
    pub(crate) exit_status: Option<ExitStatus>,
    pub(crate) process_running: bool,
    pub(crate) window_title: Option<String>,
    pub(crate) window_icon_name: Option<String>,
    pub(crate) audible_bell_count: u64,
    pub(crate) last_clipboard_copy: Option<TerminalClipboardCopy>,
    pub(crate) last_system_clipboard_text: Option<String>,
    pub(crate) last_system_clipboard_error: Option<String>,
    pub(crate) capture: bool,
    /// Set when keyboard capture was auto-released because the terminal window
    /// lost focus (e.g. a modal popup opened). Distinguishes that transient loss
    /// from an intentional release via the release shortcut, so capture can be
    /// restored automatically once focus returns.
    pub(crate) capture_suspended_by_blur: bool,
    pub(crate) release_shortcut: TerminalShortcut,
    pub(crate) prefix_shortcut: TerminalShortcut,
    pub(crate) prefix_bindings: Vec<TerminalPrefixBinding>,
    pub(crate) prefix_pending: bool,
    pub(crate) copy_mode: Option<TerminalCopyModeState>,
    pub(crate) copy_buffer: Option<String>,
    pub(crate) selection: TerminalSelectionState,
    pub(crate) command_marks: Vec<TerminalCommandBlock>,
    pub(crate) current_cwd: Option<String>,
    pub(crate) cursor_shape: TerminalCursorShape,
    pub(crate) dsr_tail: Vec<u8>,
    pub(crate) tmux_dcs_passthrough: TmuxDcsPassthroughDecoder,
}

impl TerminalShared {
    pub(crate) fn apply_runtime_config(&mut self, config: TerminalRuntimeConfig) {
        if self.scrollback_len != config.scrollback_len {
            self.scrollback_len = config.scrollback_len;
            let (rows, cols) = self.parser.screen().size();
            self.parser = terminal_parser(rows, cols, config.scrollback_len);
            self.selection.clear();
            self.copy_mode = None;
            self.command_marks.clear();
        }
        self.palette = config.palette;
        self.release_shortcut = config.release_shortcut;
        self.set_prefix_shortcut(config.prefix_shortcut);
        self.alternate_screen_scroll = config.alternate_screen_scroll;
        self.shell_integration = config.shell_integration;
        self.tmux_environment = config.tmux_environment;
        self.cursor_shape = config.cursor_shape;
    }

    pub(crate) fn set_scrollback_len(&mut self, len: usize) {
        if self.scrollback_len == len {
            return;
        }
        let (rows, cols) = self.parser.screen().size();
        self.scrollback_len = len;
        self.parser = terminal_parser(rows, cols, len);
        self.cursor_shape = TerminalCursorShape::default();
        self.selection.clear();
        self.copy_mode = None;
        self.command_marks.clear();
    }

    pub(crate) fn set_capture(&mut self, capture: bool) {
        self.capture = capture;
        if !capture {
            self.prefix_pending = false;
            self.copy_mode = None;
        }
    }

    pub(crate) fn set_prefix_shortcut(&mut self, shortcut: TerminalShortcut) {
        self.prefix_shortcut = shortcut;
        self.prefix_pending = false;
    }

    pub(crate) fn set_prefix_binding(&mut self, binding: TerminalPrefixBinding) {
        if let Some(existing) = self
            .prefix_bindings
            .iter_mut()
            .find(|existing| existing.shortcut == binding.shortcut)
        {
            *existing = binding;
        } else {
            self.prefix_bindings.push(binding);
        }
        self.prefix_pending = false;
    }

    pub(crate) fn set_prefix_bindings(
        &mut self,
        bindings: impl IntoIterator<Item = TerminalPrefixBinding>,
    ) {
        self.prefix_bindings.clear();
        for binding in bindings {
            self.set_prefix_binding(binding);
        }
        self.prefix_pending = false;
    }

    pub(crate) fn prefix_command_for_event(
        &self,
        event: KeyEvent,
    ) -> Option<TerminalPrefixCommand> {
        if self.prefix_shortcut.matches(event) {
            return Some(TerminalPrefixCommand::SendPrefix);
        }
        if event.kind == KeyEventKind::Release {
            return None;
        }
        self.prefix_bindings
            .iter()
            .find(|binding| binding.shortcut.matches(event))
            .map(|binding| binding.command)
    }

    pub(crate) fn apply_callback_events(
        &mut self,
        events: Vec<TerminalCallbackEvent>,
    ) -> Vec<TerminalCallbackDispatch> {
        let mut dispatches = Vec::new();
        for event in events {
            match event {
                TerminalCallbackEvent::WindowTitle(title) => {
                    // 某些应用 (如 Claude Code) 退出时会用 OSC 0/2 发送一个空标题来"清空"
                    // 标题。把空标题归一化为 None,表示"当前没有有效标题",这样调用方无需
                    // 自行区分空串,可直接回退到自己的默认标题。
                    self.window_title = if title.trim().is_empty() {
                        None
                    } else {
                        Some(title.clone())
                    };
                    if let Some(callback) = self.on_window_title.clone() {
                        dispatches.push(TerminalCallbackDispatch::WindowTitle(callback, title));
                    }
                }
                TerminalCallbackEvent::WindowIconName(icon_name) => {
                    self.window_icon_name = Some(icon_name.clone());
                    if let Some(callback) = self.on_window_icon_name.clone() {
                        dispatches.push(TerminalCallbackDispatch::WindowIconName(
                            callback, icon_name,
                        ));
                    }
                }
                TerminalCallbackEvent::AudibleBell => {
                    self.audible_bell_count = self.audible_bell_count.saturating_add(1);
                    if let Some(callback) = self.on_audible_bell.clone() {
                        dispatches.push(TerminalCallbackDispatch::AudibleBell(callback));
                    }
                }
                TerminalCallbackEvent::ClipboardCopy(copy) => {
                    self.last_clipboard_copy = Some(copy.clone());
                    if copy.targets_system_clipboard()
                        && let Ok(text) = copy.decoded_text()
                    {
                        self.copy_buffer = Some(text.clone());
                        dispatches.push(TerminalCallbackDispatch::SystemClipboardCopy(text));
                    }
                    if let Some(callback) = self.on_clipboard_copy.clone() {
                        dispatches.push(TerminalCallbackDispatch::ClipboardCopy(callback, copy));
                    }
                }
                TerminalCallbackEvent::UnhandledOsc { params, row, col } => {
                    if let Some(block) = self.apply_unhandled_osc(&params, row, col)
                        && let Some(callback) = self.on_command_finished.clone()
                    {
                        dispatches.push(TerminalCallbackDispatch::CommandFinished(callback, block));
                    }
                }
                TerminalCallbackEvent::CursorShape(shape) => {
                    self.cursor_shape = shape;
                }
            }
        }
        dispatches
    }

    pub(crate) fn apply_unhandled_osc(
        &mut self,
        params: &[Vec<u8>],
        row: usize,
        col: u16,
    ) -> Option<TerminalCommandBlock> {
        match params {
            [kind, rest @ ..] if kind.as_slice() == b"133" => {
                self.apply_osc133_marker(rest, row, col)
            }
            [kind, cwd] if kind.as_slice() == b"7" => {
                if let Some(cwd) = parse_osc7_cwd(cwd) {
                    self.current_cwd = Some(cwd.clone());
                    if let Some(block) = self.current_command_block_mut() {
                        block.cwd = Some(cwd);
                    }
                }
                None
            }
            _ => None,
        }
    }

    pub(crate) fn apply_osc133_marker(
        &mut self,
        params: &[Vec<u8>],
        row: usize,
        col: u16,
    ) -> Option<TerminalCommandBlock> {
        let marker = params.first().and_then(|marker| marker.first()).copied()?;
        match marker {
            b'A' => {
                self.record_prompt_start(row, col);
                None
            }
            b'B' => {
                let cwd = self.current_cwd.clone();
                let block = self.open_command_block(row, col, cwd);
                block.command_start = Some(row);
                block.command_start_col = Some(col);
                None
            }
            b'C' => {
                let cwd = self.current_cwd.clone();
                let block = self.open_command_block(row, col, cwd);
                block.output_start = Some(row);
                block.output_start_col = Some(col);
                None
            }
            b'D' => {
                let exit_code = params.get(1).and_then(|code| parse_osc133_exit_code(code));
                let cwd = self.current_cwd.clone();
                let block = self.open_command_block(row, col, cwd);
                block.end = Some(row);
                block.end_col = Some(col);
                block.exit_code = exit_code;
                Some(block.clone())
            }
            _ => None,
        }
    }

    pub(crate) fn record_prompt_start(&mut self, row: usize, col: u16) {
        let cwd = self.current_cwd.clone();
        match self.command_marks.last_mut() {
            Some(block) if block.is_open() && !block.has_command_activity() => {
                block.prompt_start = Some(row);
                block.prompt_start_col = Some(col);
                block.cwd = cwd;
            }
            _ => self
                .command_marks
                .push(TerminalCommandBlock::at_prompt(row, col, cwd)),
        }
    }

    pub(crate) fn open_command_block(
        &mut self,
        row: usize,
        col: u16,
        cwd: Option<String>,
    ) -> &mut TerminalCommandBlock {
        let needs_new_block = self
            .command_marks
            .last()
            .is_none_or(|block| !block.is_open());
        if needs_new_block {
            self.command_marks.push(TerminalCommandBlock {
                prompt_start: Some(row),
                prompt_start_col: Some(col),
                cwd,
                ..TerminalCommandBlock::default()
            });
        }
        self.command_marks.last_mut().expect("command block exists")
    }

    /// Drops command marks whose recorded rows were blanked by an in-place
    /// screen erase (Ctrl-L, the `clear`/`tput clear` command, or a full-screen
    /// app repainting the primary screen).
    ///
    /// Such an erase clears the visible rows without scrolling them into
    /// history, so the marks keep pointing at now-empty rows and paint ghost
    /// separators / output shading there. Shell integration does not re-emit an
    /// OSC 133 prompt marker on a bare Ctrl-L, so this runs after *every* output
    /// batch rather than only when a marker arrives.
    ///
    /// A mark is stale when *every* screen row it spans is blank. A live block
    /// always keeps non-blank rows (the typed command, its output), so requiring
    /// the whole span to be blank avoids false positives from a completed block
    /// whose trailing `end` row happens to be empty, or from a redraw that only
    /// repaints part of a block. A cleared block is blank throughout. Rows still
    /// in real scrollback are treated as non-blank (preserved as history).
    pub(crate) fn prune_cleared_command_marks(&mut self) {
        if self.command_marks.is_empty() {
            return;
        }
        let max_scrollback = self.max_scrollback();
        let marks = std::mem::take(&mut self.command_marks);
        let screen = self.parser.screen();
        let (rows, width) = screen.size();
        self.command_marks = marks
            .into_iter()
            .filter(|block| {
                // Only real commands (with a recorded command/output span) leave
                // ghost decorations behind after a clear. Marker-only partial
                // blocks that legitimately sit on an empty line are preserved.
                if !block.has_command_activity() {
                    return true;
                }
                let (Some(anchor), Some(last)) = (block.anchor_row(), block.last_row()) else {
                    return true;
                };
                (anchor..=last).any(|absolute_row| {
                    // A row outside the live viewport (still in scrollback, or
                    // past the bottom) counts as non-blank so the mark survives.
                    let Some(visible) = absolute_row.checked_sub(max_scrollback) else {
                        return true;
                    };
                    match u16::try_from(visible) {
                        Ok(visible) if visible < rows => {
                            !command_mark_row_is_blank(screen, visible, width)
                        }
                        _ => true,
                    }
                })
            })
            .collect();
    }

    pub(crate) fn current_command_block_mut(&mut self) -> Option<&mut TerminalCommandBlock> {
        self.command_marks
            .last_mut()
            .filter(|block| block.is_open())
    }

    pub(crate) fn queue_input(&mut self, bytes: &[u8]) {
        self.input.extend(bytes);
    }

    pub(crate) fn max_scrollback(&mut self) -> usize {
        let screen = self.parser.screen_mut();
        let current = screen.scrollback();
        screen.set_scrollback(usize::MAX);
        let max = screen.scrollback();
        screen.set_scrollback(current);
        max
    }

    pub(crate) fn scrollback_offset(&self) -> usize {
        self.parser.screen().scrollback()
    }

    pub(crate) fn set_scrollback_offset(&mut self, offset: usize) {
        self.parser.screen_mut().set_scrollback(offset);
    }

    pub(crate) fn resize_screen(&mut self, rows: u16, cols: u16) -> bool {
        let screen = self.parser.screen_mut();
        if screen.size() == (rows, cols) {
            return false;
        }
        screen.set_size(rows, cols);
        true
    }

    pub(crate) fn set_scrollback_from_scroll_offset(&mut self, scroll_offset: u16) {
        let max = self.max_scrollback().min(u16::MAX as usize);
        let y = scroll_offset.min(max as u16) as usize;
        let offset = max.saturating_sub(y);
        self.set_scrollback_offset(offset);
    }

    pub(crate) fn enter_copy_mode(&mut self) {
        let cursor = self.current_copy_mode_position();
        self.copy_mode = Some(TerminalCopyModeState::new(cursor));
        self.prefix_pending = false;
        self.selection.clear();
        self.ensure_copy_mode_cursor_visible();
    }

    pub(crate) fn cancel_copy_mode(&mut self) {
        self.copy_mode = None;
        self.selection.clear();
    }

    pub(crate) fn finish_copy_mode_copy(&mut self) -> Option<String> {
        let text = self.copy_selection();
        self.copy_mode = None;
        self.selection.clear();
        text
    }

    pub(crate) fn copy_selection(&mut self) -> Option<String> {
        let text = self.selected_text()?;
        self.copy_buffer = Some(text.clone());
        Some(text)
    }

    pub(crate) fn paste_copy_buffer_bytes(&self) -> Option<Vec<u8>> {
        self.copy_buffer
            .as_deref()
            .map(|text| encode_paste_text(self.parser.screen(), text))
    }

    pub(crate) fn selected_text(&mut self) -> Option<String> {
        let range = self.selection.range()?;
        let max_scrollback = self.max_scrollback();
        selected_text_from_screen(self.parser.screen_mut(), max_scrollback, range)
    }

    pub(crate) fn command_block_index_at_position(
        &self,
        position: TerminalSelectionPosition,
    ) -> Option<usize> {
        self.command_marks
            .iter()
            .enumerate()
            .rev()
            .find(|(_, block)| block.contains_row(position.row))
            .map(|(index, _)| index)
    }

    pub(crate) fn scroll_to_command_block(&mut self, index: usize) -> bool {
        let Some(anchor_row) = self
            .command_marks
            .get(index)
            .and_then(TerminalCommandBlock::anchor_row)
        else {
            return false;
        };
        let max = self.max_scrollback();
        let target_top = anchor_row.min(max);
        let desired = max.saturating_sub(target_top);
        if self.parser.screen().scrollback() == desired {
            return false;
        }
        self.parser.screen_mut().set_scrollback(desired);
        true
    }

    pub(crate) fn scroll_to_previous_command_block(&mut self) -> Option<usize> {
        let max = self.max_scrollback();
        let current_top = visible_top_row(max, self.parser.screen().scrollback());
        let target = if self.parser.screen().scrollback() == 0 {
            self.command_marks
                .iter()
                .enumerate()
                .rev()
                .find(|(_, block)| block.anchor_row().is_some_and(|row| row < max))
        } else {
            self.command_marks
                .iter()
                .enumerate()
                .rev()
                .find(|(_, block)| block.anchor_row().is_some_and(|row| row < current_top))
        };
        let (index, _) = target?;
        self.scroll_to_command_block(index).then_some(index)
    }

    pub(crate) fn scroll_to_next_command_block(&mut self) -> Option<usize> {
        let max = self.max_scrollback();
        let current_top = visible_top_row(max, self.parser.screen().scrollback());
        let (index, _) = self
            .command_marks
            .iter()
            .enumerate()
            .find(|(_, block)| block.anchor_row().is_some_and(|row| row > current_top))?;
        self.scroll_to_command_block(index).then_some(index)
    }

    pub(crate) fn select_command_block_output(
        &mut self,
        index: usize,
    ) -> Option<TerminalSelectionRange> {
        let range = self.command_block_text_range(index, CommandBlockTextKind::Output)?;
        self.selection.start_keyboard(range.start);
        self.selection.update(range.end);
        Some(range)
    }

    pub(crate) fn copy_command_block_text(
        &mut self,
        index: usize,
        kind: CommandBlockTextKind,
    ) -> Option<String> {
        let text = self.command_block_text(index, kind)?;
        self.copy_buffer = Some(text.clone());
        Some(text)
    }

    pub(crate) fn command_block_rerun_bytes(&mut self, index: usize) -> Option<Vec<u8>> {
        let command = self.command_block_text(index, CommandBlockTextKind::Command)?;
        let mut bytes = command.into_bytes();
        bytes.push(b'\n');
        Some(bytes)
    }

    pub(crate) fn command_block_text(
        &mut self,
        index: usize,
        kind: CommandBlockTextKind,
    ) -> Option<String> {
        let range = self.command_block_text_range(index, kind)?;
        let max_scrollback = self.max_scrollback();
        let text = selected_text_from_screen(self.parser.screen_mut(), max_scrollback, range)?;
        trim_terminal_block_text(text)
    }

    pub(crate) fn command_block_text_range(
        &mut self,
        index: usize,
        kind: CommandBlockTextKind,
    ) -> Option<TerminalSelectionRange> {
        let block = self.command_marks.get(index)?.clone();
        let (rows, cols) = self.parser.screen().size();
        let max_scrollback = self.max_scrollback();
        let bottom_row = max_scrollback
            .saturating_add(usize::from(rows))
            .saturating_sub(1);
        match kind {
            CommandBlockTextKind::Command => {
                let end_row = block.output_start.or(block.end)?;
                let end_col = if block.output_start.is_some() {
                    block.output_start_col.unwrap_or(cols)
                } else {
                    block.end_col.unwrap_or(cols)
                };

                // The command-start marker (OSC 133 `B`) is only a usable start
                // when it precedes the output-start marker (`C`). Real shell
                // integrations (zsh preexec, bash PS0) emit `B` and `C` together
                // *after* the user submits the command, so they land at the same
                // position and the `B..C` range collapses to empty. In that case
                // the typed command lives on the prompt line, so fall back to the
                // prompt-start marker.
                let command_start = block.command_start.filter(|&cmd_row| {
                    cmd_row < end_row
                        || (cmd_row == end_row && block.command_start_col.unwrap_or(0) < end_col)
                });
                let (start_row, start_col) = match command_start {
                    Some(row) => (row, block.command_start_col.unwrap_or(0)),
                    None => (block.prompt_start?, block.prompt_start_col.unwrap_or(0)),
                };

                TerminalSelectionRange::new(
                    TerminalSelectionPosition::new(start_row, start_col),
                    TerminalSelectionPosition::new(end_row, end_col),
                )
            }
            CommandBlockTextKind::Output => {
                let start_row = block.output_start?;
                let start_col = block.output_start_col.unwrap_or(0);
                let end_row = block.end.unwrap_or(bottom_row);
                let end_col = block.end_col.unwrap_or(cols);
                TerminalSelectionRange::new(
                    TerminalSelectionPosition::new(start_row, start_col),
                    TerminalSelectionPosition::new(end_row, end_col),
                )
            }
        }
    }

    pub(crate) fn current_copy_mode_position(&mut self) -> TerminalSelectionPosition {
        let max_scrollback = self.max_scrollback();
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let (row, col) = screen.cursor_position();
        position_for_view_cell(
            max_scrollback,
            screen.scrollback(),
            rows,
            cols,
            row.min(rows.saturating_sub(1)),
            col.min(cols),
        )
    }

    pub(crate) fn begin_copy_mode_selection(&mut self) {
        let Some(cursor) = self.copy_mode.as_ref().map(|mode| mode.cursor) else {
            return;
        };
        self.selection.start_keyboard(cursor);
        if let Some(mode) = &mut self.copy_mode {
            mode.selecting = true;
        }
    }

    pub(crate) fn move_copy_mode_cursor(&mut self, row_delta: isize, col_delta: isize) -> bool {
        let Some(cursor) = self.copy_mode.as_ref().map(|mode| mode.cursor) else {
            return false;
        };
        let row = if row_delta.is_negative() {
            cursor.row.saturating_sub(row_delta.unsigned_abs())
        } else {
            cursor.row.saturating_add(row_delta as usize)
        };
        let col = if col_delta.is_negative() {
            cursor
                .col
                .saturating_sub(col_delta.unsigned_abs().min(u16::MAX as usize) as u16)
        } else {
            cursor
                .col
                .saturating_add((col_delta as usize).min(u16::MAX as usize) as u16)
        };
        self.set_copy_mode_cursor(TerminalSelectionPosition::new(row, col))
    }

    pub(crate) fn set_copy_mode_cursor(&mut self, position: TerminalSelectionPosition) -> bool {
        let position = self.clamp_copy_mode_position(position);
        let Some(mode) = &mut self.copy_mode else {
            return false;
        };
        if mode.cursor == position {
            return false;
        }
        mode.cursor = position;
        if mode.selecting {
            self.selection.update(position);
        }
        self.ensure_copy_mode_cursor_visible();
        true
    }

    pub(crate) fn move_copy_mode_cursor_to_column(&mut self, col: u16) -> bool {
        let Some(cursor) = self.copy_mode.as_ref().map(|mode| mode.cursor) else {
            return false;
        };
        self.set_copy_mode_cursor(TerminalSelectionPosition::new(cursor.row, col))
    }

    pub(crate) fn move_copy_mode_cursor_by_page(&mut self, page_delta: isize) -> bool {
        let rows = self.parser.screen().size().0.max(1) as isize;
        self.move_copy_mode_cursor(page_delta.saturating_mul(rows), 0)
    }

    pub(crate) fn clamp_copy_mode_position(
        &mut self,
        position: TerminalSelectionPosition,
    ) -> TerminalSelectionPosition {
        let max_scrollback = self.max_scrollback();
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let last_row = max_scrollback.saturating_add(usize::from(rows.saturating_sub(1)));
        TerminalSelectionPosition::new(position.row.min(last_row), position.col.min(cols))
    }

    pub(crate) fn ensure_copy_mode_cursor_visible(&mut self) {
        let Some(cursor) = self.copy_mode.as_ref().map(|mode| mode.cursor) else {
            return;
        };
        let max_scrollback = self.max_scrollback();
        let screen = self.parser.screen();
        let (rows, _) = screen.size();
        if rows == 0 {
            return;
        }
        let current = screen.scrollback();
        let top = visible_top_row(max_scrollback, current);
        let height = usize::from(rows);
        let bottom = top.saturating_add(height.saturating_sub(1));
        let desired = if cursor.row < top {
            max_scrollback.saturating_sub(cursor.row)
        } else if cursor.row > bottom {
            max_scrollback.saturating_sub(cursor.row.saturating_sub(height.saturating_sub(1)))
        } else {
            current
        };
        if desired != current {
            self.parser
                .screen_mut()
                .set_scrollback(desired.min(max_scrollback));
        }
    }

    pub(crate) fn scroll_copy_mode_view(&mut self, line_delta: isize) -> bool {
        if self.copy_mode.is_none() {
            return false;
        }
        let max = self.max_scrollback();
        let current = self.parser.screen().scrollback();
        let desired = if line_delta.is_negative() {
            current.saturating_sub(line_delta.unsigned_abs())
        } else {
            current.saturating_add(line_delta as usize).min(max)
        };
        if desired != current {
            self.parser.screen_mut().set_scrollback(desired);
            self.clamp_copy_mode_cursor_to_visible();
            return true;
        }
        false
    }

    pub(crate) fn clamp_copy_mode_cursor_to_visible(&mut self) {
        let Some(cursor) = self.copy_mode.as_ref().map(|mode| mode.cursor) else {
            return;
        };
        let max_scrollback = self.max_scrollback();
        let screen = self.parser.screen();
        let (rows, _) = screen.size();
        if rows == 0 {
            return;
        }
        let top = visible_top_row(max_scrollback, screen.scrollback());
        let bottom = top.saturating_add(usize::from(rows.saturating_sub(1)));
        let row = cursor.row.clamp(top, bottom);
        if row != cursor.row {
            let _ = self.set_copy_mode_cursor(TerminalSelectionPosition::new(row, cursor.col));
        }
    }
}
