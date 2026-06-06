use ratatui::style::Color;

use atto_ui::composable::ScrollbarVisibility;
use atto_ui::reactive::Binding;

use super::{LinkCallback, MarkdownCache, MarkdownShared};

impl MarkdownShared {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        markdown: Binding<String>,
        width: Binding<Option<u16>>,
        show_markers: Binding<bool>,
        streaming_tolerant: Binding<bool>,
        vertical_scrollbar: Binding<ScrollbarVisibility>,
        max_code_height: Binding<u16>,
        max_table_height: Binding<u16>,
        fg_override: Binding<Option<Color>>,
        bg_override: Binding<Option<Color>>,
        link_callback: LinkCallback,
    ) -> Self {
        let cache = MarkdownCache::new(
            &markdown,
            &show_markers,
            &streaming_tolerant,
            &max_code_height,
            &max_table_height,
        );

        Self {
            markdown,
            width,
            show_markers,
            streaming_tolerant,
            vertical_scrollbar,
            max_code_height,
            max_table_height,
            fg_override,
            bg_override,
            link_callback,
            cache,
            embedded_scrollbar_drag: None,
        }
    }

    pub(super) fn resolve_wrap_width(&self, viewport_width: u16) -> u16 {
        match self.width.get() {
            Some(w) => w.min(viewport_width),
            None => viewport_width,
        }
    }

    pub(super) fn ensure_layout(&mut self, wrap_width: u16) {
        self.cache.ensure_layout(
            &self.markdown,
            &self.show_markers,
            &self.streaming_tolerant,
            &self.max_code_height,
            &self.max_table_height,
            wrap_width,
        );
    }
}

impl MarkdownCache {
    fn new(
        markdown: &Binding<String>,
        show_markers: &Binding<bool>,
        streaming_tolerant: &Binding<bool>,
        max_code_height: &Binding<u16>,
        max_table_height: &Binding<u16>,
    ) -> Self {
        Self {
            md_dirty: markdown.dirty_observer(),
            markers_dirty: show_markers.dirty_observer(),
            streaming_tolerant_dirty: streaming_tolerant.dirty_observer(),
            max_code_dirty: max_code_height.dirty_observer(),
            max_table_dirty: max_table_height.dirty_observer(),
            parsed: Vec::new(),
            code_blocks: Vec::new(),
            tables: Vec::new(),
            layout: None,
            last_wrap_width: None,
            last_markdown: String::new(),
        }
    }

    fn ensure_layout(
        &mut self,
        markdown: &Binding<String>,
        show_markers: &Binding<bool>,
        streaming_tolerant: &Binding<bool>,
        max_code_height: &Binding<u16>,
        max_table_height: &Binding<u16>,
        wrap_width: u16,
    ) {
        let markdown_changed = markdown.check_dirty(&mut self.md_dirty);
        let markers_changed = show_markers.check_dirty(&mut self.markers_dirty);
        let streaming_changed = streaming_tolerant.check_dirty(&mut self.streaming_tolerant_dirty);
        let code_height_changed = max_code_height.check_dirty(&mut self.max_code_dirty);
        let table_height_changed = max_table_height.check_dirty(&mut self.max_table_dirty);
        let width_changed = self.last_wrap_width != Some(wrap_width);

        if markdown_changed || markers_changed || streaming_changed || self.layout.is_none() {
            let markdown_text = markdown.get();
            let show_markers = show_markers.get();
            let streaming_tolerant = streaming_tolerant.get();
            let updated_incrementally = markdown_changed
                && !markers_changed
                && !streaming_changed
                && streaming_tolerant
                && self.try_update_unclosed_code_block(&markdown_text);
            if !updated_incrementally {
                self.parsed = if streaming_tolerant {
                    super::parser::parse_markdown_tolerant(&markdown_text, show_markers)
                } else {
                    super::parser::parse_markdown(&markdown_text, show_markers)
                };
                let (codes, tables) = super::parser::build_block_states(&self.parsed);
                self.code_blocks = codes;
                self.tables = tables;
            }
            self.last_markdown = markdown_text;
        }

        if markdown_changed
            || markers_changed
            || streaming_changed
            || code_height_changed
            || table_height_changed
            || width_changed
            || self.layout.is_none()
        {
            let max_code_height = max_code_height.get();
            let max_table_height = max_table_height.get();
            let show_markers = show_markers.get();
            let layout = super::layout::build_layout(
                &self.parsed,
                wrap_width,
                max_code_height,
                max_table_height,
                show_markers,
                &self.code_blocks,
                &self.tables,
            );
            self.layout = Some(layout);
            self.last_wrap_width = Some(wrap_width);
        }
    }

    fn try_update_unclosed_code_block(&mut self, markdown_text: &str) -> bool {
        if self.last_markdown.is_empty() || !markdown_text.starts_with(&self.last_markdown) {
            return false;
        }
        let Some(previous) = super::parser::unclosed_fenced_code_block(&self.last_markdown) else {
            return false;
        };
        let Some(next) = super::parser::unclosed_fenced_code_block(markdown_text) else {
            return false;
        };
        if previous.info != next.info {
            return false;
        }
        if !super::parser::replace_last_code_block_text(&mut self.parsed, next.text) {
            return false;
        }
        let (codes, tables) = super::parser::build_block_states(&self.parsed);
        self.code_blocks = codes;
        self.tables = tables;
        true
    }
}
