use std::sync::Arc;

use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEventKind};
use parking_lot::RwLock;
use pulldown_cmark::{Alignment, CodeBlockKind, Event as MdEvent, Options, Parser, Tag, TagEnd};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::reactive::{Binding, DirtyObserver};
use crate::theme::Theme;
use crate::view::{View, ViewContext, ViewEventResult};
use crate::views::{ScrollConfig, ScrollContent, ScrollContentContext, ScrollView, ScrollViewHost};
use crate::views::{ScrollOffset, ScrollbarVisibility};

/// Markdown viewer view with clickable links, word-wrapping, and optional vertical scrolling.
///
/// This component intentionally preserves "format markers" in the rendered output (e.g. `#`,
/// `*`, backticks, `>`, list bullets, and link `[...] (url)` syntax).
pub struct MarkdownViewer {
    markdown: Binding<String>,
    width: Binding<u16>,
    vertical_scrollbar: Binding<ScrollbarVisibility>,
    code_block_max_height: Binding<u16>,
    table_max_height: Binding<u16>,
    style_overrides: Binding<StyleOverrides>,
    link_handler: Arc<LinkHandler>,
    scroll_config: Binding<ScrollConfig>,
    inner: ScrollView,
    vertical_scrollbar_dirty: DirtyObserver,
}

impl MarkdownViewer {
    pub fn new(markdown: impl Into<Binding<String>>) -> Self {
        Self::new_with_width(markdown, 60u16)
    }

    pub fn new_with_width(
        markdown: impl Into<Binding<String>>,
        width: impl Into<Binding<u16>>,
    ) -> Self {
        let markdown: Binding<String> = markdown.into();
        let width: Binding<u16> = width.into();
        let vertical_scrollbar: Binding<ScrollbarVisibility> = ScrollbarVisibility::Auto.into();
        let code_block_max_height: Binding<u16> = 8u16.into();
        let table_max_height: Binding<u16> = 8u16.into();
        let style_overrides: Binding<StyleOverrides> = StyleOverrides::default().into();
        let link_handler = Arc::new(LinkHandler::default());

        let scroll_config: Binding<ScrollConfig> = ScrollConfig::default()
            .vertical_scrollbar(vertical_scrollbar.get())
            .horizontal_scrollbar(ScrollbarVisibility::Never)
            .into();

        let content = MarkdownContent::new(
            markdown.clone(),
            width.clone(),
            vertical_scrollbar.clone(),
            code_block_max_height.clone(),
            table_max_height.clone(),
            style_overrides.clone(),
            Arc::clone(&link_handler),
        );
        let inner = ScrollView::new(Box::new(content)).with_scroll_config(scroll_config.clone());

        Self {
            markdown,
            width,
            vertical_scrollbar_dirty: vertical_scrollbar.dirty_observer(),
            vertical_scrollbar,
            code_block_max_height,
            table_max_height,
            style_overrides,
            link_handler,
            scroll_config,
            inner,
        }
    }

    pub fn width(mut self, width: impl Into<Binding<u16>>) -> Self {
        self.width = width.into();
        self.rebuild_inner();
        self
    }

    pub fn vertical_scrollbar(mut self, vis: impl Into<Binding<ScrollbarVisibility>>) -> Self {
        self.vertical_scrollbar = vis.into();
        self.vertical_scrollbar_dirty = self.vertical_scrollbar.dirty_observer();
        self.force_refresh_scroll_config();
        self.rebuild_inner();
        self
    }

    pub fn on_link_click<F>(self, cb: F) -> Self
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        self.link_handler
            .set(Some(Arc::new(cb) as Arc<dyn Fn(String) + Send + Sync>));
        self
    }

    pub fn code_block_max_height(mut self, height: impl Into<Binding<u16>>) -> Self {
        self.code_block_max_height = height.into();
        self.rebuild_inner();
        self
    }

    pub fn table_max_height(mut self, height: impl Into<Binding<u16>>) -> Self {
        self.table_max_height = height.into();
        self.rebuild_inner();
        self
    }

    pub fn background(self, color: Color) -> Self {
        let mut overrides = self.style_overrides.get();
        overrides.base = Some(overrides.base.unwrap_or_default().bg(color));
        self.style_overrides.set(overrides);
        self
    }

    pub fn text_color(self, color: Color) -> Self {
        let mut overrides = self.style_overrides.get();
        overrides.text = Some(overrides.text.unwrap_or_default().fg(color));
        self.style_overrides.set(overrides);
        self
    }

    fn refresh_scroll_config(&mut self) {
        if !self
            .vertical_scrollbar
            .check_dirty(&mut self.vertical_scrollbar_dirty)
        {
            return;
        }

        let mut cfg = self.scroll_config.get();
        cfg.vertical_scrollbar = self.vertical_scrollbar.get();
        cfg.horizontal_scrollbar = ScrollbarVisibility::Never;
        self.scroll_config.set(cfg);
    }

    fn force_refresh_scroll_config(&mut self) {
        let mut cfg = self.scroll_config.get();
        cfg.vertical_scrollbar = self.vertical_scrollbar.get();
        cfg.horizontal_scrollbar = ScrollbarVisibility::Never;
        self.scroll_config.set(cfg);
    }

    fn rebuild_inner(&mut self) {
        let content = MarkdownContent::new(
            self.markdown.clone(),
            self.width.clone(),
            self.vertical_scrollbar.clone(),
            self.code_block_max_height.clone(),
            self.table_max_height.clone(),
            self.style_overrides.clone(),
            Arc::clone(&self.link_handler),
        );
        self.inner = ScrollView::new(Box::new(content)).with_scroll_config(self.scroll_config.clone());
    }
}

impl View for MarkdownViewer {
    fn desired_width(&self) -> Option<u16> {
        // Ensure the ScrollView's desired width remains stable.
        self.inner.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.inner.desired_height()
    }

    fn is_scrollable(&self) -> bool {
        self.inner.is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        self.inner.content_size()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.inner.viewport_size()
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.inner.scroll_config()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.inner.scroll_offset()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.inner.set_scroll_offset(x, y);
    }

    fn handle_event(&mut self, event: &Event, ctx: ViewContext<'_>) -> ViewEventResult {
        self.refresh_scroll_config();
        self.inner.handle_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.refresh_scroll_config();
        self.inner.draw(frame, area, ctx);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct StyleOverrides {
    base: Option<Style>,
    text: Option<Style>,
}

#[derive(Default)]
struct LinkHandler {
    cb: RwLock<Option<Arc<dyn Fn(String) + Send + Sync>>>,
}

impl LinkHandler {
    fn set(&self, cb: Option<Arc<dyn Fn(String) + Send + Sync>>) {
        *self.cb.write() = cb;
    }

    fn call(&self, url: String) {
        if let Some(cb) = self.cb.read().as_ref() {
            cb(url);
        }
    }
}

struct MarkdownContent {
    markdown: Binding<String>,
    width: Binding<u16>,
    vertical_scrollbar: Binding<ScrollbarVisibility>,
    code_block_max_height: Binding<u16>,
    table_max_height: Binding<u16>,
    style_overrides: Binding<StyleOverrides>,
    link_handler: Arc<LinkHandler>,

    markdown_dirty: DirtyObserver,
    width_dirty: DirtyObserver,
    vertical_dirty: DirtyObserver,
    code_block_max_height_dirty: DirtyObserver,
    table_max_height_dirty: DirtyObserver,

    layout: Option<MarkdownLayout>,
}

impl MarkdownContent {
    fn new(
        markdown: Binding<String>,
        width: Binding<u16>,
        vertical_scrollbar: Binding<ScrollbarVisibility>,
        code_block_max_height: Binding<u16>,
        table_max_height: Binding<u16>,
        style_overrides: Binding<StyleOverrides>,
        link_handler: Arc<LinkHandler>,
    ) -> Self {
        Self {
            markdown_dirty: markdown.dirty_observer(),
            width_dirty: width.dirty_observer(),
            vertical_dirty: vertical_scrollbar.dirty_observer(),
            code_block_max_height_dirty: code_block_max_height.dirty_observer(),
            table_max_height_dirty: table_max_height.dirty_observer(),

            markdown,
            width,
            vertical_scrollbar,
            code_block_max_height,
            table_max_height,
            style_overrides,
            link_handler,
            layout: None,
        }
    }

    fn ensure_layout(&mut self, viewport_width: u16) {
        let markdown_changed = self.markdown.check_dirty(&mut self.markdown_dirty);
        let width_changed = self.width.check_dirty(&mut self.width_dirty);
        let vertical_changed = self.vertical_scrollbar.check_dirty(&mut self.vertical_dirty);
        let code_h_changed = self
            .code_block_max_height
            .check_dirty(&mut self.code_block_max_height_dirty);
        let table_h_changed = self.table_max_height.check_dirty(&mut self.table_max_height_dirty);

        let needs_rebuild = self
            .layout
            .as_ref()
            .is_none_or(|l| l.viewport_width != viewport_width)
            || markdown_changed
            || width_changed
            || vertical_changed
            || code_h_changed
            || table_h_changed;

        if !needs_rebuild {
            return;
        }

        let prev_scroll = (!markdown_changed)
            .then(|| self.layout.as_ref().map(MarkdownLayout::scrollable_states))
            .flatten()
            .unwrap_or_default();

        let md = self.markdown.get();
        let doc = parse_markdown_document(&md);
        let code_max = self.code_block_max_height.get().max(1);
        let table_max = self.table_max_height.get().max(1);
        let mut layout = MarkdownLayout::build(&doc, viewport_width, code_max, table_max);
        layout.apply_scrollable_states(&prev_scroll);
        self.layout = Some(layout);
    }
}

impl ScrollContent for MarkdownContent {
    fn desired_width(&self) -> Option<u16> {
        Some(self.width.get().max(1))
    }

    fn desired_height(&self) -> Option<u16> {
        if !matches!(self.vertical_scrollbar.get(), ScrollbarVisibility::Never) {
            return None;
        }
        // In "no vertical scrollbar" mode, size to content (based on the configured width).
        let viewport_width = self.width.get().max(1);
        let md = self.markdown.get();
        let doc = parse_markdown_document(&md);
        let code_max = self.code_block_max_height.get().max(1);
        let table_max = self.table_max_height.get().max(1);
        let layout = MarkdownLayout::build(&doc, viewport_width, code_max, table_max);
        Some(layout.content_height)
    }

    fn content_size(
        &mut self,
        viewport: (u16, u16),
        ctx: ScrollContentContext<'_>,
    ) -> (u16, u16) {
        let viewport_width = viewport.0;
        self.ensure_layout(viewport_width);
        let content_h = self.layout.as_ref().map(|l| l.content_height).unwrap_or(0);

        // Ensure the background extends fully across the viewport.
        let content_w = viewport_width.max(1);

        // If we have no content, still return a 1x0 or 1x1? Keep height 0 so parents can collapse.
        let _ = ctx;
        (content_w, content_h)
    }

    fn handle_event(
        &mut self,
        event: &Event,
        ctx: ScrollContentContext<'_>,
        _host: &mut ScrollViewHost,
    ) -> ViewEventResult {
        let viewport_width = ctx.info.viewport_size.0;
        self.ensure_layout(viewport_width);

        let Some(layout) = self.layout.as_mut() else {
            return ViewEventResult::ignored();
        };

        let scroll = ctx.info.scroll_offset;
        match event {
            Event::Mouse(m) => {
                // Local coordinates are within the ScrollView content rect.
                let local_x = m.column;
                let local_y = m.row;

                // Translate into "document" coordinates.
                let doc_x = scroll.x.saturating_add(local_x);
                let doc_y = scroll.y.saturating_add(local_y);

                // Scrollable inner blocks get first shot at wheel events.
                //
                // Crossterm's mouse support is inconsistent across terminals/PTYS for
                // `ScrollLeft`/`ScrollRight`. However, many terminals emit Shift+wheel as
                // horizontal scrolling, while still reporting the event as `ScrollUp`/`ScrollDown`
                // with `KeyModifiers::SHIFT`.
                let kind = match m.kind {
                    MouseEventKind::ScrollDown if m.modifiers.contains(KeyModifiers::SHIFT) => {
                        MouseEventKind::ScrollRight
                    }
                    MouseEventKind::ScrollUp if m.modifiers.contains(KeyModifiers::SHIFT) => {
                        MouseEventKind::ScrollLeft
                    }
                    other => other,
                };

                match kind {
                    MouseEventKind::ScrollUp
                    | MouseEventKind::ScrollDown
                    | MouseEventKind::ScrollLeft
                    | MouseEventKind::ScrollRight => {
                        if let Some(changed) = layout.handle_wheel(doc_x, doc_y, kind) {
                            return if changed {
                                ViewEventResult::consumed()
                            } else {
                                ViewEventResult::ignored()
                            };
                        }
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        if let Some(url) = layout.hit_test_link(doc_x, doc_y) {
                            self.link_handler.call(url.to_string());
                            return ViewEventResult::consumed();
                        }
                    }
                    _ => {}
                }
                ViewEventResult::ignored()
            }
            _ => ViewEventResult::ignored(),
        }
    }

    fn draw(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ScrollContentContext<'_>,
        _host: &mut ScrollViewHost,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let viewport_width = area.width;
        self.ensure_layout(viewport_width);

        let overrides = self.style_overrides.get();
        let base = MarkdownStyles::resolve_base(ctx.view.theme, overrides);
        let styles = MarkdownStyles::resolve(ctx.view.theme, overrides);

        // Clear/fill background.
        fill_rect(frame.buffer_mut(), area, base);

        let Some(layout) = self.layout.as_ref() else {
            return;
        };

        let scroll = ctx.info.scroll_offset;
        layout.draw(frame.buffer_mut(), area, scroll, &styles);
    }
}

// ---- Style resolution -------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
struct MarkdownStyles {
    base: Style,
    text: Style,
    marker: Style,
    list_marker: Style,
    blockquote: Style,
    heading: [Style; 6],
    strong: Style,
    emphasis: Style,
    strikethrough: Style,
    code_inline: Style,
    link: Style,
    link_url: Style,
    code_block: Style,
    code_block_border: Style,
    table_border: Style,
    table_header: Style,
    table_cell: Style,
}

impl MarkdownStyles {
    fn resolve_base(theme: &Theme, overrides: StyleOverrides) -> Style {
        let base = theme
            .named_style("markdown-base")
            .unwrap_or(theme.window_bg);
        base.patch(overrides.base.unwrap_or_default())
    }

    fn resolve(theme: &Theme, overrides: StyleOverrides) -> Self {
        let base = Self::resolve_base(theme, overrides);
        let text = base
            .patch(theme.named_style("markdown-text").unwrap_or(theme.widget.normal))
            .patch(overrides.text.unwrap_or_default());

        let marker = text.patch(theme.named_style("markdown-marker").unwrap_or(theme.widget.dim));
        let list_marker =
            text.patch(theme.named_style("markdown-list-marker").unwrap_or(theme.widget.dim));
        let blockquote =
            text.patch(theme.named_style("markdown-blockquote").unwrap_or(theme.widget.dim));

        let mut heading = [Style::default(); 6];
        for (idx, slot) in heading.iter_mut().enumerate() {
            let key = format!("markdown-heading-{}", idx + 1);
            *slot = text.patch(
                theme.named_style(&key)
                    .unwrap_or(theme.widget.focused.add_modifier(Modifier::BOLD)),
            );
        }

        let strong = text.patch(
            theme.named_style("markdown-strong")
                .unwrap_or(Style::default().add_modifier(Modifier::BOLD)),
        );
        let emphasis = text.patch(
            theme.named_style("markdown-emphasis")
                .unwrap_or(Style::default().add_modifier(Modifier::ITALIC)),
        );
        let strikethrough = text.patch(
            theme.named_style("markdown-strikethrough")
                .unwrap_or(Style::default().add_modifier(Modifier::CROSSED_OUT)),
        );
        let code_inline = text.patch(
            theme.named_style("markdown-code-inline")
                .unwrap_or(Style::default().add_modifier(Modifier::REVERSED)),
        );
        let link = text.patch(
            theme.named_style("markdown-link")
                .unwrap_or(theme.widget.accent.add_modifier(Modifier::UNDERLINED)),
        );
        let link_url = text.patch(theme.named_style("markdown-link-url").unwrap_or(theme.widget.dim));
        let code_block = text.patch(theme.named_style("markdown-code-block").unwrap_or(Style::default()));
        let code_block_border =
            base.patch(theme.named_style("markdown-code-border").unwrap_or(theme.window_border));

        let table_border = base.patch(
            theme.named_style("markdown-table-border")
                .unwrap_or(theme.window_border),
        );
        let table_header =
            text.patch(theme.named_style("markdown-table-header").unwrap_or(theme.widget.accent));
        let table_cell =
            text.patch(theme.named_style("markdown-table-cell").unwrap_or(theme.widget.normal));

        Self {
            base,
            text,
            marker,
            list_marker,
            blockquote,
            heading,
            strong,
            emphasis,
            strikethrough,
            code_inline,
            link,
            link_url,
            code_block,
            code_block_border,
            table_border,
            table_header,
            table_cell,
        }
    }

    fn style_for_segment(&self, seg: &StyledSegment) -> Style {
        let mut style = match seg.block_context {
            BlockContext::Normal => self.text,
            BlockContext::Heading(level) => {
                let idx = level.saturating_sub(1).min(5) as usize;
                self.heading[idx]
            }
            BlockContext::BlockQuote => self.blockquote,
            BlockContext::CodeBlock => self.code_block,
            BlockContext::TableBorder => self.table_border,
            BlockContext::TableHeader => self.table_header,
            BlockContext::TableCell => self.table_cell,
        };

        if seg.inline.strong {
            style = style.patch(self.strong);
        }
        if seg.inline.emphasis {
            style = style.patch(self.emphasis);
        }
        if seg.inline.strikethrough {
            style = style.patch(self.strikethrough);
        }
        if seg.inline.code_inline {
            style = style.patch(self.code_inline);
        }

        if seg.link.is_some() {
            style = style.patch(self.link);
        }
        if seg.is_link_url {
            style = style.patch(self.link_url);
        }

        if let SegmentKind::Marker(m) = seg.kind && !matches!(seg.block_context, BlockContext::TableBorder) {
            style = match m {
                MarkerKind::List => style.patch(self.list_marker),
                MarkerKind::BlockQuote => style.patch(self.blockquote),
                _ => style.patch(self.marker),
            };
        }

        style
    }
}

// ---- Layout model -----------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct InlineFlags {
    strong: bool,
    emphasis: bool,
    strikethrough: bool,
    code_inline: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SegmentKind {
    Text,
    Marker(MarkerKind),
    LineBreak,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkerKind {
    General,
    Heading,
    List,
    BlockQuote,
    Link,
    Code,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockContext {
    Normal,
    Heading(u8),
    BlockQuote,
    CodeBlock,
    TableBorder,
    TableHeader,
    TableCell,
}

#[derive(Clone, Debug)]
struct StyledSegment {
    text: String,
    kind: SegmentKind,
    block_context: BlockContext,
    inline: InlineFlags,
    link: Option<Arc<str>>,
    is_link_url: bool,
}

impl StyledSegment {
    fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: SegmentKind::Text,
            block_context: BlockContext::Normal,
            inline: InlineFlags::default(),
            link: None,
            is_link_url: false,
        }
    }

    fn marker(text: impl Into<String>, kind: MarkerKind) -> Self {
        Self {
            text: text.into(),
            kind: SegmentKind::Marker(kind),
            block_context: BlockContext::Normal,
            inline: InlineFlags::default(),
            link: None,
            is_link_url: false,
        }
    }

    fn line_break() -> Self {
        Self {
            text: "\n".into(),
            kind: SegmentKind::LineBreak,
            block_context: BlockContext::Normal,
            inline: InlineFlags::default(),
            link: None,
            is_link_url: false,
        }
    }

    fn with_block_context(mut self, ctx: BlockContext) -> Self {
        self.block_context = ctx;
        self
    }

    fn with_inline(mut self, inline: InlineFlags) -> Self {
        self.inline = inline;
        self
    }

    fn with_link(mut self, link: Option<Arc<str>>) -> Self {
        self.link = link;
        self
    }

    fn as_link_url(mut self) -> Self {
        self.is_link_url = true;
        self
    }
}

#[derive(Clone, Debug, Default)]
struct StyledLine {
    segments: Vec<StyledSegment>,
}

impl StyledLine {
    fn new(segments: Vec<StyledSegment>) -> Self {
        Self { segments }
    }
}

#[derive(Clone, Debug)]
enum LayoutBlock {
    Text { y: u16, lines: Vec<StyledLine> },
    CodeBlock { y: u16, block: ScrollableBlock },
    Table { y: u16, block: ScrollableBlock },
}

#[derive(Clone, Debug)]
struct ScrollableBlock {
    kind: ScrollableKind,
    viewport_height: u16,
    scroll_x: u16,
    scroll_y: u16,
    content_width: u16,
    content_height: u16,
    lines: Vec<StyledLine>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrollableKind {
    CodeBlock { has_border: bool },
    Table,
}

#[derive(Clone, Copy, Debug)]
struct ScrollableBlockState {
    kind: ScrollableKind,
    scroll_x: u16,
    scroll_y: u16,
}

impl ScrollableBlock {
    fn height(&self) -> u16 {
        match self.kind {
            ScrollableKind::CodeBlock { has_border: true } => self.viewport_height.saturating_add(2),
            _ => self.viewport_height,
        }
    }

    fn inner_viewport(&self, outer_width: u16) -> (u16, u16) {
        match self.kind {
            ScrollableKind::CodeBlock { has_border: true } => (
                outer_width.saturating_sub(2),
                self.viewport_height,
            ),
            _ => (outer_width, self.viewport_height),
        }
    }

    fn clamp_scroll(&mut self, outer_width: u16) {
        let (vw, vh) = self.inner_viewport(outer_width);
        let max_x = self.content_width.saturating_sub(vw);
        let max_y = self.content_height.saturating_sub(vh);
        self.scroll_x = self.scroll_x.min(max_x);
        self.scroll_y = self.scroll_y.min(max_y);
    }
}

#[derive(Clone, Debug)]
struct MarkdownLayout {
    viewport_width: u16,
    content_height: u16,
    blocks: Vec<LayoutBlock>,
}

impl MarkdownLayout {
    fn build(doc: &[MdBlock], viewport_width: u16, code_max_h: u16, table_max_h: u16) -> Self {
        let viewport_width = viewport_width.max(1);

        let mut builder = LayoutBuilder::new(viewport_width, code_max_h, table_max_h);
        builder.render_blocks(doc, RenderContext::default());
        builder.finish()
    }

    fn draw(&self, buf: &mut Buffer, area: Rect, scroll: ScrollOffset, styles: &MarkdownStyles) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let view_top = scroll.y;
        let view_bottom = scroll.y.saturating_add(area.height);

        for block in &self.blocks {
            match block {
                LayoutBlock::Text { y, lines } => {
                    let block_top = *y;
                    let block_bottom = y.saturating_add(lines.len().min(u16::MAX as usize) as u16);
                    if block_bottom <= view_top || block_top >= view_bottom {
                        continue;
                    }

                    for (idx, line) in lines.iter().enumerate() {
                        let row = y.saturating_add(idx as u16);
                        if row < view_top || row >= view_bottom {
                            continue;
                        }

                        let screen_y = area.y.saturating_add(row.saturating_sub(view_top));
                        draw_styled_line(
                            buf,
                            area.x,
                            screen_y,
                            area.width,
                            0,
                            line,
                            styles,
                        );
                    }
                }
                LayoutBlock::CodeBlock { y, block } => {
                    self.draw_scrollable_block(buf, area, scroll, styles, *y, block);
                }
                LayoutBlock::Table { y, block } => {
                    self.draw_scrollable_block(buf, area, scroll, styles, *y, block);
                }
            }
        }
    }

    fn draw_scrollable_block(
        &self,
        buf: &mut Buffer,
        area: Rect,
        scroll: ScrollOffset,
        styles: &MarkdownStyles,
        y: u16,
        block: &ScrollableBlock,
    ) {
        let view_top = scroll.y;
        let view_bottom = scroll.y.saturating_add(area.height);

        let block_top = y;
        let block_height = block.height();
        let block_bottom = y.saturating_add(block_height);
        if block_bottom <= view_top || block_top >= view_bottom {
            return;
        }

        match block.kind {
            ScrollableKind::CodeBlock { has_border: true } => {
                self.draw_code_block_with_border(buf, area, scroll, styles, y, block);
            }
            _ => {
                self.draw_plain_scrollable_lines(buf, area, scroll, styles, y, block);
            }
        }
    }

    fn draw_code_block_with_border(
        &self,
        buf: &mut Buffer,
        area: Rect,
        scroll: ScrollOffset,
        styles: &MarkdownStyles,
        y: u16,
        block: &ScrollableBlock,
    ) {
        let view_top = scroll.y;

        let outer_w = area.width;
        let border = styles.code_block_border;
        let (inner_w, inner_h) = block.inner_viewport(outer_w);

        // Draw border + inner content row-by-row with proper clipping.
        for dy in 0..block.height() {
            let row = y.saturating_add(dy);
            let screen_y = area.y.saturating_add(row.saturating_sub(view_top));
            if screen_y < area.y || screen_y >= area.y.saturating_add(area.height) {
                continue;
            }

            let is_top = dy == 0;
            let is_bottom = dy + 1 == block.height();

            if outer_w == 0 {
                continue;
            }

            if is_top || is_bottom {
                // Top/bottom border line.
                let (left, mid, right) = border_glyphs(is_top);
                if outer_w == 1 {
                    buf[(area.x, screen_y)].set_symbol(left).set_style(border);
                    continue;
                }

                buf[(area.x, screen_y)].set_symbol(left).set_style(border);
                for dx in 1..outer_w.saturating_sub(1) {
                    buf[(area.x.saturating_add(dx), screen_y)]
                        .set_symbol(mid)
                        .set_style(border);
                }
                buf[(area.x.saturating_add(outer_w.saturating_sub(1)), screen_y)]
                    .set_symbol(right)
                    .set_style(border);
                continue;
            }

            // Middle rows: side borders + inner content.
            let side = "│";
            buf[(area.x, screen_y)].set_symbol(side).set_style(border);
            if outer_w > 1 {
                buf[(area.x.saturating_add(outer_w.saturating_sub(1)), screen_y)]
                    .set_symbol(side)
                    .set_style(border);
            }

            // Inner area fill.
            let inner_x = area.x.saturating_add(1);
            let inner_y_idx = dy.saturating_sub(1);

            // Clear inner row.
            for dx in 0..inner_w {
                buf[(inner_x.saturating_add(dx), screen_y)]
                    .set_symbol(" ")
                    .set_style(styles.code_block);
            }

            if inner_y_idx >= inner_h {
                continue;
            }

            let line_idx = block.scroll_y.saturating_add(inner_y_idx);
            let Some(line) = block.lines.get(line_idx as usize) else {
                continue;
            };
            draw_styled_line(
                buf,
                inner_x,
                screen_y,
                inner_w,
                block.scroll_x,
                line,
                styles,
            );
        }
    }

    fn draw_plain_scrollable_lines(
        &self,
        buf: &mut Buffer,
        area: Rect,
        scroll: ScrollOffset,
        styles: &MarkdownStyles,
        y: u16,
        block: &ScrollableBlock,
    ) {
        let view_top = scroll.y;
        let view_bottom = scroll.y.saturating_add(area.height);

        let outer_w = area.width;

        for dy in 0..block.viewport_height {
            let row = y.saturating_add(dy);
            if row < view_top || row >= view_bottom {
                continue;
            }
            let screen_y = area.y.saturating_add(row.saturating_sub(view_top));
            // Clear row background for table-style blocks so the viewport looks consistent.
            for dx in 0..outer_w {
                buf[(area.x.saturating_add(dx), screen_y)]
                    .set_symbol(" ")
                    .set_style(styles.base);
            }

            let line_idx = block.scroll_y.saturating_add(dy);
            let Some(line) = block.lines.get(line_idx as usize) else {
                continue;
            };
            draw_styled_line(
                buf,
                area.x,
                screen_y,
                outer_w,
                block.scroll_x,
                line,
                styles,
            );
        }
    }

    fn scrollable_states(&self) -> Vec<ScrollableBlockState> {
        let mut out = Vec::new();
        for block in &self.blocks {
            match block {
                LayoutBlock::CodeBlock { block, .. } | LayoutBlock::Table { block, .. } => out.push(
                    ScrollableBlockState {
                        kind: block.kind,
                        scroll_x: block.scroll_x,
                        scroll_y: block.scroll_y,
                    },
                ),
                LayoutBlock::Text { .. } => {}
            }
        }
        out
    }

    fn apply_scrollable_states(&mut self, states: &[ScrollableBlockState]) {
        if states.is_empty() {
            return;
        }

        let mut idx = 0usize;
        for block in &mut self.blocks {
            let block_ref = match block {
                LayoutBlock::CodeBlock { block, .. } | LayoutBlock::Table { block, .. } => block,
                LayoutBlock::Text { .. } => continue,
            };

            let Some(state) = states.get(idx) else {
                break;
            };
            idx += 1;

            if !scrollable_kind_compatible(block_ref.kind, state.kind) {
                // If the markdown structure changed, avoid carrying scroll positions over to a
                // different block type.
                continue;
            }

            block_ref.scroll_x = state.scroll_x;
            block_ref.scroll_y = state.scroll_y;
            block_ref.clamp_scroll(self.viewport_width);
        }
    }

    fn handle_wheel(
        &mut self,
        doc_x: u16,
        doc_y: u16,
        kind: MouseEventKind,
    ) -> Option<bool> {
        for block in &mut self.blocks {
            let (y, block_ref) = match block {
                LayoutBlock::CodeBlock { y, block } => (*y, block),
                LayoutBlock::Table { y, block } => (*y, block),
                _ => continue,
            };

            let height = block_ref.height();
            if doc_y < y || doc_y >= y.saturating_add(height) {
                continue;
            }

            // Horizontal bounds.
            // - For now, treat the full viewport width as the hit target.
            let _ = doc_x;

            let before = (block_ref.scroll_x, block_ref.scroll_y);

            let step: i16 = 3;
            match kind {
                MouseEventKind::ScrollUp => {
                    block_ref.scroll_y = add_signed(block_ref.scroll_y, -step);
                }
                MouseEventKind::ScrollDown => {
                    block_ref.scroll_y = add_signed(block_ref.scroll_y, step);
                }
                MouseEventKind::ScrollLeft => {
                    block_ref.scroll_x = add_signed(block_ref.scroll_x, -step);
                }
                MouseEventKind::ScrollRight => {
                    block_ref.scroll_x = add_signed(block_ref.scroll_x, step);
                }
                _ => {}
            }

            block_ref.clamp_scroll(self.viewport_width);
            let after = (block_ref.scroll_x, block_ref.scroll_y);
            return Some(before != after);
        }
        None
    }

    fn hit_test_link(&self, doc_x: u16, doc_y: u16) -> Option<Arc<str>> {
        for block in &self.blocks {
            match block {
                LayoutBlock::Text { y, lines } => {
                    let rel = doc_y.saturating_sub(*y);
                    let Some(line) = lines.get(rel as usize) else {
                        continue;
                    };
                    if let Some(url) = hit_test_line_link(line, doc_x) {
                        return Some(url);
                    }
                }
                LayoutBlock::Table { y, block } => {
                    let rel = doc_y.saturating_sub(*y);
                    if rel >= block.viewport_height {
                        continue;
                    }
                    let line_idx = block.scroll_y.saturating_add(rel);
                    let Some(line) = block.lines.get(line_idx as usize) else {
                        continue;
                    };
                    let x = doc_x.saturating_add(block.scroll_x);
                    if let Some(url) = hit_test_line_link(line, x) {
                        return Some(url);
                    }
                }
                LayoutBlock::CodeBlock { .. } => {}
            }
        }
        None
    }
}

fn scrollable_kind_compatible(a: ScrollableKind, b: ScrollableKind) -> bool {
    match (a, b) {
        (ScrollableKind::Table, ScrollableKind::Table) => true,
        (ScrollableKind::CodeBlock { .. }, ScrollableKind::CodeBlock { .. }) => true,
        _ => false,
    }
}

struct LayoutBuilder {
    viewport_width: u16,
    code_block_max_height: u16,
    table_max_height: u16,

    blocks: Vec<LayoutBlock>,
    pending_text: Vec<StyledLine>,
    cursor_y: u16,
}

impl LayoutBuilder {
    fn new(viewport_width: u16, code_max_h: u16, table_max_h: u16) -> Self {
        Self {
            viewport_width,
            code_block_max_height: code_max_h,
            table_max_height: table_max_h,
            blocks: Vec::new(),
            pending_text: Vec::new(),
            cursor_y: 0,
        }
    }

    fn finish(mut self) -> MarkdownLayout {
        self.flush_text();
        MarkdownLayout {
            viewport_width: self.viewport_width,
            content_height: self.cursor_y,
            blocks: self.blocks,
        }
    }

    fn flush_text(&mut self) {
        if self.pending_text.is_empty() {
            return;
        }
        let start = self.cursor_y.saturating_sub(self.pending_text.len() as u16);
        let lines = std::mem::take(&mut self.pending_text);
        self.blocks.push(LayoutBlock::Text { y: start, lines });
    }

    fn push_blank_line(&mut self) {
        self.pending_text.push(StyledLine::default());
        self.cursor_y = self.cursor_y.saturating_add(1);
    }

    fn push_lines(&mut self, lines: Vec<StyledLine>) {
        if lines.is_empty() {
            return;
        }
        self.cursor_y = self.cursor_y.saturating_add(lines.len() as u16);
        self.pending_text.extend(lines);
    }

    fn push_code_block(&mut self, mut block: ScrollableBlock) {
        self.flush_text();
        block.clamp_scroll(self.viewport_width);
        let y = self.cursor_y;
        self.cursor_y = self.cursor_y.saturating_add(block.height());
        self.blocks.push(LayoutBlock::CodeBlock { y, block });
    }

    fn push_table(&mut self, mut block: ScrollableBlock) {
        self.flush_text();
        block.clamp_scroll(self.viewport_width);
        let y = self.cursor_y;
        self.cursor_y = self.cursor_y.saturating_add(block.height());
        self.blocks.push(LayoutBlock::Table { y, block });
    }

    fn render_blocks(&mut self, blocks: &[MdBlock], ctx: RenderContext) {
        let mut first = true;
        for block in blocks {
            if !first {
                self.push_blank_line();
            }
            first = false;
            self.render_block(block, ctx);
        }
    }

    fn render_block(&mut self, block: &MdBlock, ctx: RenderContext) {
        match block {
            MdBlock::Heading { level, inlines } => {
                let prefix = ctx.base_prefix();
                let hashes = "#".repeat((*level).max(1) as usize);
                let marker = StyledSegment::marker(format!("{hashes} "), MarkerKind::Heading)
                    .with_block_context(BlockContext::Heading(*level));

                let mut first_prefix = prefix.clone();
                first_prefix.push(marker);

                let cont_spaces = " ".repeat(cell_width(&format!("{hashes} ")).min(64) as usize);
                let mut cont_prefix = prefix;
                cont_prefix.push(StyledSegment::text(cont_spaces).with_block_context(BlockContext::Heading(*level)));

                let mut segs: Vec<StyledSegment> = Vec::new();
                for s in inlines {
                    segs.push(s.clone().with_block_context(BlockContext::Heading(*level)));
                }

                let lines = wrap_segments(&segs, self.viewport_width, &first_prefix, &cont_prefix);
                self.push_lines(lines);
            }
            MdBlock::Paragraph { inlines } => {
                let prefix = ctx.base_prefix();
                let mut segs: Vec<StyledSegment> = Vec::new();
                for s in inlines {
                    segs.push(s.clone().with_block_context(ctx.text_block_context()));
                }
                let lines = wrap_segments(&segs, self.viewport_width, &prefix, &prefix);
                self.push_lines(lines);
            }
            MdBlock::BlockQuote { blocks } => {
                let next_ctx = RenderContext {
                    blockquote_depth: ctx.blockquote_depth.saturating_add(1),
                    indent: ctx.indent,
                };
                self.render_blocks(blocks, next_ctx);
            }
            MdBlock::List { ordered, start, items } => {
                for (idx, item) in items.iter().enumerate() {
                    if idx > 0 {
                        // Lists should not insert a blank line between items by default.
                        // (Users can control extra spacing in the markdown source itself.)
                    }
                    let marker_text = if *ordered {
                        format!("{}. ", start.saturating_add(idx as u64))
                    } else {
                        "- ".to_string()
                    };
                    let marker_width = cell_width(&marker_text);

                    let base_prefix = ctx.base_prefix();

                    let mut first_prefix = base_prefix.clone();
                    first_prefix.push(StyledSegment::marker(marker_text.clone(), MarkerKind::List));

                    let mut cont_prefix = base_prefix;
                    cont_prefix.push(StyledSegment::text(" ".repeat(marker_width as usize)));

                    // Render item blocks with increased indent.
                    let item_ctx = RenderContext {
                        blockquote_depth: ctx.blockquote_depth,
                        indent: ctx.indent.saturating_add(marker_width),
                    };

                    let mut first_block = true;
                    for b in item {
                        if !first_block {
                            self.push_blank_line();
                        }
                        first_block = false;

                        match b {
                            MdBlock::Paragraph { inlines } => {
                                let mut segs: Vec<StyledSegment> = Vec::new();
                                for s in inlines {
                                    segs.push(s.clone().with_block_context(item_ctx.text_block_context()));
                                }
                                let lines =
                                    wrap_segments(&segs, self.viewport_width, &first_prefix, &cont_prefix);
                                self.push_lines(lines);
                            }
                            MdBlock::Heading { level, inlines } => {
                                let hashes = "#".repeat((*level).max(1) as usize);
                                let heading_marker =
                                    StyledSegment::marker(format!("{hashes} "), MarkerKind::Heading)
                                        .with_block_context(BlockContext::Heading(*level));
                                let mut heading_first = first_prefix.clone();
                                heading_first.push(heading_marker);

                                let cont_spaces = " ".repeat(cell_width(&format!("{hashes} ")).min(64) as usize);
                                let mut heading_cont = cont_prefix.clone();
                                heading_cont.push(StyledSegment::text(cont_spaces).with_block_context(BlockContext::Heading(*level)));

                                let mut segs: Vec<StyledSegment> = Vec::new();
                                for s in inlines {
                                    segs.push(s.clone().with_block_context(BlockContext::Heading(*level)));
                                }

                                let lines =
                                    wrap_segments(&segs, self.viewport_width, &heading_first, &heading_cont);
                                self.push_lines(lines);
                            }
                            _ => {
                                // Nested structures (blockquote/lists/code/tables) should be
                                // aligned under the list item content.
                                self.render_block(b, item_ctx);
                            }
                        }
                    }
                }
            }
            MdBlock::CodeBlock { language, content } => {
                // Spacing around code blocks.
                // (Callers already insert blank lines between blocks, so only keep this tight.)

                let mut lines: Vec<StyledLine> = Vec::new();
                let fence = if let Some(lang) = language.as_deref()
                    && !lang.trim().is_empty()
                {
                    format!("```{lang}")
                } else {
                    "```".to_string()
                };
                lines.push(StyledLine::new(vec![
                    StyledSegment::marker(fence, MarkerKind::Code).with_block_context(BlockContext::CodeBlock),
                ]));

                for line in content.lines() {
                    lines.push(StyledLine::new(vec![StyledSegment::text(line).with_block_context(BlockContext::CodeBlock)]));
                }

                lines.push(StyledLine::new(vec![
                    StyledSegment::marker("```", MarkerKind::Code).with_block_context(BlockContext::CodeBlock),
                ]));

                let content_height = lines.len().min(u16::MAX as usize) as u16;
                let content_width = lines
                    .iter()
                    .map(|l| line_width_cells(l))
                    .max()
                    .unwrap_or(0);

                let viewport_height = content_height.min(self.code_block_max_height.max(1));

                let block = ScrollableBlock {
                    kind: ScrollableKind::CodeBlock { has_border: true },
                    viewport_height,
                    scroll_x: 0,
                    scroll_y: 0,
                    content_width,
                    content_height,
                    lines,
                };
                self.push_code_block(block);
            }
            MdBlock::Table { header, rows } => {
                let lines = render_table_lines(header, rows);
                let content_height = lines.len().min(u16::MAX as usize) as u16;
                let content_width = lines.iter().map(line_width_cells).max().unwrap_or(0);

                let viewport_height = content_height.min(self.table_max_height.max(1));
                let block = ScrollableBlock {
                    kind: ScrollableKind::Table,
                    viewport_height,
                    scroll_x: 0,
                    scroll_y: 0,
                    content_width,
                    content_height,
                    lines,
                };
                self.push_table(block);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct RenderContext {
    blockquote_depth: u8,
    indent: u16,
}

impl RenderContext {
    fn base_prefix(self) -> Vec<StyledSegment> {
        let mut out = Vec::new();

        for _ in 0..self.blockquote_depth {
            out.push(StyledSegment::marker("> ", MarkerKind::BlockQuote).with_block_context(BlockContext::BlockQuote));
        }

        if self.indent > 0 {
            out.push(StyledSegment::text(" ".repeat(self.indent as usize)));
        }

        out
    }

    fn text_block_context(self) -> BlockContext {
        if self.blockquote_depth > 0 {
            BlockContext::BlockQuote
        } else {
            BlockContext::Normal
        }
    }
}

// ---- Markdown parsing -------------------------------------------------------

#[derive(Clone, Debug)]
enum MdBlock {
    Heading { level: u8, inlines: Vec<StyledSegment> },
    Paragraph { inlines: Vec<StyledSegment> },
    BlockQuote { blocks: Vec<MdBlock> },
    List {
        ordered: bool,
        start: u64,
        items: Vec<Vec<MdBlock>>,
    },
    CodeBlock {
        language: Option<String>,
        content: String,
    },
    Table {
        header: Vec<Vec<StyledSegment>>,
        rows: Vec<Vec<Vec<StyledSegment>>>,
    },
}

fn heading_level_to_u8(level: pulldown_cmark::HeadingLevel) -> u8 {
    match level {
        pulldown_cmark::HeadingLevel::H1 => 1,
        pulldown_cmark::HeadingLevel::H2 => 2,
        pulldown_cmark::HeadingLevel::H3 => 3,
        pulldown_cmark::HeadingLevel::H4 => 4,
        pulldown_cmark::HeadingLevel::H5 => 5,
        pulldown_cmark::HeadingLevel::H6 => 6,
    }
}

fn parse_markdown_document(input: &str) -> Vec<MdBlock> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let mut stack: Vec<BlockContainer> = vec![BlockContainer::Root { blocks: Vec::new() }];
    let mut inline_stack: Vec<InlineContainer> = Vec::new();

    let mut in_table_head = false;

    for ev in Parser::new_ext(input, options) {
        // Inline tags are routed to the active inline builder when present.
        let has_inline = !inline_stack.is_empty();

        match ev {
            MdEvent::Start(tag) => match tag {
                Tag::Paragraph => inline_stack.push(InlineContainer::paragraph()),
                Tag::Heading { level, .. } => {
                    inline_stack.push(InlineContainer::heading(heading_level_to_u8(level)))
                }
                Tag::BlockQuote => stack.push(BlockContainer::BlockQuote { blocks: Vec::new() }),
                Tag::List(start) => stack.push(BlockContainer::List {
                    ordered: start.is_some(),
                    start: start.unwrap_or(1),
                    items: Vec::new(),
                }),
                Tag::Item => stack.push(BlockContainer::ListItem { blocks: Vec::new() }),
                Tag::CodeBlock(kind) => {
                    let language = match kind {
                        CodeBlockKind::Fenced(lang) => {
                            let s = lang.to_string();
                            (!s.trim().is_empty()).then_some(s)
                        }
                        CodeBlockKind::Indented => None,
                    };
                    stack.push(BlockContainer::CodeBlock {
                        language,
                        content: String::new(),
                    });
                }
                Tag::Table(alignments) => stack.push(BlockContainer::Table {
                    _alignments: alignments,
                    header: Vec::new(),
                    rows: Vec::new(),
                }),
                Tag::TableHead => {
                    in_table_head = true;
                }
                Tag::TableRow => stack.push(BlockContainer::TableRow {
                    in_head: in_table_head,
                    cells: Vec::new(),
                }),
                Tag::TableCell => inline_stack.push(InlineContainer::table_cell()),

                // Inline-only tags: forward to the inline builder when present.
                Tag::Emphasis
                | Tag::Strong
                | Tag::Strikethrough
                | Tag::Link { .. }
                | Tag::Image { .. } => {
                    if let Some(inline) = inline_stack.last_mut() {
                        inline.handle_start_tag(tag);
                    }
                }
                _ => {
                    if has_inline {
                        if let Some(inline) = inline_stack.last_mut() {
                            inline.handle_start_tag(tag);
                        }
                    }
                }
            },
            MdEvent::End(tag) => match tag {
                TagEnd::Paragraph => {
                    if let Some(inline) = inline_stack.pop()
                        && let Some(blocks) = inline.finish_into_block()
                    {
                        stack_push_block(&mut stack, blocks);
                    }
                }
                TagEnd::Heading(level) => {
                    if let Some(inline) = inline_stack.pop()
                        && let Some(blocks) =
                            inline.finish_into_block_with_heading_level(heading_level_to_u8(level))
                    {
                        stack_push_block(&mut stack, blocks);
                    }
                }
                TagEnd::BlockQuote => {
                    let BlockContainer::BlockQuote { blocks } =
                        stack.pop().unwrap_or(BlockContainer::Root { blocks: Vec::new() })
                    else {
                        continue;
                    };
                    stack_push_block(&mut stack, MdBlock::BlockQuote { blocks });
                }
                TagEnd::Item => {
                    let BlockContainer::ListItem { blocks } =
                        stack.pop().unwrap_or(BlockContainer::ListItem { blocks: Vec::new() })
                    else {
                        continue;
                    };
                    // Attach to the closest list container.
                    if let Some(BlockContainer::List { items, .. }) = stack.last_mut() {
                        items.push(blocks);
                    }
                }
                TagEnd::List(_) => {
                    let BlockContainer::List {
                        ordered,
                        start,
                        items,
                    } = stack.pop().unwrap_or(BlockContainer::List {
                        ordered: false,
                        start: 1,
                        items: Vec::new(),
                    }) else {
                        continue;
                    };
                    stack_push_block(
                        &mut stack,
                        MdBlock::List {
                            ordered,
                            start,
                            items,
                        },
                    );
                }
                TagEnd::CodeBlock => {
                    let BlockContainer::CodeBlock { language, content } =
                        stack.pop().unwrap_or(BlockContainer::CodeBlock {
                            language: None,
                            content: String::new(),
                        })
                    else {
                        continue;
                    };
                    stack_push_block(&mut stack, MdBlock::CodeBlock { language, content });
                }
                TagEnd::TableHead => {
                    in_table_head = false;
                }
                TagEnd::TableRow => {
                    let BlockContainer::TableRow { in_head, cells } =
                        stack.pop().unwrap_or(BlockContainer::TableRow {
                            in_head: false,
                            cells: Vec::new(),
                        })
                    else {
                        continue;
                    };
                    if let Some(BlockContainer::Table { header, rows, .. }) = stack.last_mut() {
                        if in_head {
                            *header = cells;
                        } else {
                            rows.push(cells);
                        }
                    }
                }
                TagEnd::Table => {
                    let BlockContainer::Table { header, rows, .. } =
                        stack.pop().unwrap_or(BlockContainer::Table {
                            _alignments: Vec::new(),
                            header: Vec::new(),
                            rows: Vec::new(),
                        })
                    else {
                        continue;
                    };
                    stack_push_block(&mut stack, MdBlock::Table { header, rows });
                }
                TagEnd::TableCell => {
                    if let Some(inline) = inline_stack.pop() {
                        let cell = inline.finish_into_inlines();
                        if let Some(BlockContainer::TableRow { cells, .. }) = stack.last_mut() {
                            cells.push(cell);
                        }
                    }
                }

                // Inline tags.
                TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Link
                | TagEnd::Image => {
                    if let Some(inline) = inline_stack.last_mut() {
                        inline.handle_end_tag(tag);
                    }
                }
                _ => {
                    if let Some(inline) = inline_stack.last_mut() {
                        inline.handle_end_tag(tag);
                    }
                }
            },
            MdEvent::Text(t) => {
                if let Some(inline) = inline_stack.last_mut() {
                    inline.push_text(&t);
                    continue;
                }
                // Code blocks use Text events too.
                if let Some(BlockContainer::CodeBlock { content, .. }) = stack.last_mut() {
                    content.push_str(&t);
                }
            }
            MdEvent::Code(t) => {
                if let Some(inline) = inline_stack.last_mut() {
                    inline.push_inline_code(&t);
                }
            }
            MdEvent::SoftBreak => {
                if let Some(inline) = inline_stack.last_mut() {
                    inline.push_text(" ");
                }
            }
            MdEvent::HardBreak => {
                if let Some(inline) = inline_stack.last_mut() {
                    inline.push_line_break();
                }
            }
            MdEvent::Html(_)
            | MdEvent::InlineHtml(_)
            | MdEvent::FootnoteReference(_)
            | MdEvent::Rule
            | MdEvent::TaskListMarker(_) => {
                // Ignore (not part of MVP for this component).
            }
        }
    }

    // Flush any remaining root blocks.
    let BlockContainer::Root { blocks } = stack.into_iter().next().unwrap_or(BlockContainer::Root {
        blocks: Vec::new(),
    }) else {
        return Vec::new();
    };
    blocks
}

#[derive(Clone, Debug)]
enum BlockContainer {
    Root { blocks: Vec<MdBlock> },
    BlockQuote { blocks: Vec<MdBlock> },
    List {
        ordered: bool,
        start: u64,
        items: Vec<Vec<MdBlock>>,
    },
    ListItem { blocks: Vec<MdBlock> },
    CodeBlock { language: Option<String>, content: String },
    Table {
        _alignments: Vec<Alignment>,
        header: Vec<Vec<StyledSegment>>,
        rows: Vec<Vec<Vec<StyledSegment>>>,
    },
    TableRow { in_head: bool, cells: Vec<Vec<StyledSegment>> },
}

fn stack_push_block(stack: &mut [BlockContainer], block: MdBlock) {
    if let Some(last) = stack.last_mut() {
        match last {
            BlockContainer::Root { blocks } => blocks.push(block),
            BlockContainer::BlockQuote { blocks } => blocks.push(block),
            BlockContainer::ListItem { blocks } => blocks.push(block),
            // Blocks should not normally be pushed directly into these.
            BlockContainer::List { .. }
            | BlockContainer::CodeBlock { .. }
            | BlockContainer::Table { .. }
            | BlockContainer::TableRow { .. } => {}
        }
    }
}

#[derive(Clone, Debug)]
enum InlineContainerKind {
    Paragraph,
    Heading { level: u8 },
    TableCell,
}

#[derive(Clone, Debug)]
struct InlineContainer {
    kind: InlineContainerKind,
    builder: InlineBuilder,
}

impl InlineContainer {
    fn paragraph() -> Self {
        Self {
            kind: InlineContainerKind::Paragraph,
            builder: InlineBuilder::new(),
        }
    }

    fn heading(level: u8) -> Self {
        Self {
            kind: InlineContainerKind::Heading { level },
            builder: InlineBuilder::new(),
        }
    }

    fn table_cell() -> Self {
        Self {
            kind: InlineContainerKind::TableCell,
            builder: InlineBuilder::new(),
        }
    }

    fn handle_start_tag(&mut self, tag: Tag<'_>) {
        self.builder.handle_start_tag(tag);
    }

    fn handle_end_tag(&mut self, tag: TagEnd) {
        self.builder.handle_end_tag(tag);
    }

    fn push_text(&mut self, text: &str) {
        self.builder.push_text(text);
    }

    fn push_inline_code(&mut self, code: &str) {
        self.builder.push_inline_code(code);
    }

    fn push_line_break(&mut self) {
        self.builder.push_line_break();
    }

    fn finish_into_block(self) -> Option<MdBlock> {
        match self.kind {
            InlineContainerKind::Paragraph => Some(MdBlock::Paragraph {
                inlines: self.builder.finish(),
            }),
            InlineContainerKind::Heading { level } => Some(MdBlock::Heading {
                level,
                inlines: self.builder.finish(),
            }),
            InlineContainerKind::TableCell => None,
        }
    }

    fn finish_into_block_with_heading_level(self, level: u8) -> Option<MdBlock> {
        // Prefer the level from the closing tag to stay consistent with pulldown-cmark.
        let _ = self.kind;
        Some(MdBlock::Heading {
            level,
            inlines: self.builder.finish(),
        })
    }

    fn finish_into_inlines(self) -> Vec<StyledSegment> {
        self.builder.finish()
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct InlineDepth {
    strong: u8,
    emphasis: u8,
    strikethrough: u8,
}

#[derive(Clone, Debug, Default)]
struct InlineBuilder {
    segments: Vec<StyledSegment>,
    depth: InlineDepth,
    link_stack: Vec<Arc<str>>,
}

impl InlineBuilder {
    fn new() -> Self {
        Self::default()
    }

    fn current_link(&self) -> Option<Arc<str>> {
        self.link_stack.last().cloned()
    }

    fn flags(&self) -> InlineFlags {
        InlineFlags {
            strong: self.depth.strong > 0,
            emphasis: self.depth.emphasis > 0,
            strikethrough: self.depth.strikethrough > 0,
            code_inline: false,
        }
    }

    fn push_seg(&mut self, seg: StyledSegment) {
        self.segments.push(seg);
    }

    fn handle_start_tag(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Emphasis => {
                self.push_seg(
                    StyledSegment::marker("*", MarkerKind::General)
                        .with_inline(self.flags())
                        .with_link(self.current_link()),
                );
                self.depth.emphasis = self.depth.emphasis.saturating_add(1);
            }
            Tag::Strong => {
                self.push_seg(
                    StyledSegment::marker("**", MarkerKind::General)
                        .with_inline(self.flags())
                        .with_link(self.current_link()),
                );
                self.depth.strong = self.depth.strong.saturating_add(1);
            }
            Tag::Strikethrough => {
                self.push_seg(
                    StyledSegment::marker("~~", MarkerKind::General)
                        .with_inline(self.flags())
                        .with_link(self.current_link()),
                );
                self.depth.strikethrough = self.depth.strikethrough.saturating_add(1);
            }
            Tag::Link { dest_url, .. } => {
                let url: Arc<str> = Arc::from(dest_url.to_string());
                self.push_seg(
                    StyledSegment::marker("[", MarkerKind::Link)
                        .with_inline(self.flags())
                        .with_link(Some(Arc::clone(&url))),
                );
                self.link_stack.push(url);
            }
            Tag::Image { .. } => {
                // For MVP, treat images as their alt text (handled via Text events).
            }
            _ => {}
        }
    }

    fn handle_end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Emphasis => {
                self.depth.emphasis = self.depth.emphasis.saturating_sub(1);
                self.push_seg(
                    StyledSegment::marker("*", MarkerKind::General)
                        .with_inline(self.flags())
                        .with_link(self.current_link()),
                );
            }
            TagEnd::Strong => {
                self.depth.strong = self.depth.strong.saturating_sub(1);
                self.push_seg(
                    StyledSegment::marker("**", MarkerKind::General)
                        .with_inline(self.flags())
                        .with_link(self.current_link()),
                );
            }
            TagEnd::Strikethrough => {
                self.depth.strikethrough = self.depth.strikethrough.saturating_sub(1);
                self.push_seg(
                    StyledSegment::marker("~~", MarkerKind::General)
                        .with_inline(self.flags())
                        .with_link(self.current_link()),
                );
            }
            TagEnd::Link => {
                let url = self.link_stack.pop();
                if let Some(url) = url {
                    self.push_seg(
                        StyledSegment::marker("](", MarkerKind::Link)
                            .with_inline(self.flags())
                            .with_link(Some(Arc::clone(&url))),
                    );
                    self.push_seg(
                        StyledSegment::text(url.to_string())
                            .with_inline(self.flags())
                            .with_link(Some(Arc::clone(&url)))
                            .as_link_url(),
                    );
                    self.push_seg(
                        StyledSegment::marker(")", MarkerKind::Link)
                            .with_inline(self.flags())
                            .with_link(Some(Arc::clone(&url))),
                    );
                } else {
                    self.push_seg(StyledSegment::marker("]", MarkerKind::Link));
                }
            }
            _ => {}
        }
    }

    fn push_text(&mut self, text: &str) {
        self.push_seg(
            StyledSegment::text(text)
                .with_inline(self.flags())
                .with_link(self.current_link()),
        );
    }

    fn push_inline_code(&mut self, code: &str) {
        self.push_seg(
            StyledSegment::marker("`", MarkerKind::Code)
                .with_inline(self.flags())
                .with_link(self.current_link()),
        );

        let mut st = self.flags();
        st.code_inline = true;
        self.push_seg(
            StyledSegment::text(code)
                .with_inline(st)
                .with_link(self.current_link())
                .with_block_context(BlockContext::Normal),
        );

        self.push_seg(
            StyledSegment::marker("`", MarkerKind::Code)
                .with_inline(self.flags())
                .with_link(self.current_link()),
        );
    }

    fn push_line_break(&mut self) {
        self.push_seg(StyledSegment::line_break());
    }

    fn finish(self) -> Vec<StyledSegment> {
        self.segments
    }
}

// ---- Wrapping + drawing helpers --------------------------------------------

#[derive(Clone, Debug)]
struct Atom {
    g: String,
    w: u16,
    seg: StyledSegment,
    is_space: bool,
    is_newline: bool,
}

fn segments_to_atoms(segments: &[StyledSegment]) -> Vec<Atom> {
    let mut out = Vec::new();
    for seg in segments {
        if matches!(seg.kind, SegmentKind::LineBreak) {
            out.push(Atom {
                g: "\n".into(),
                w: 0,
                seg: seg.clone(),
                is_space: false,
                is_newline: true,
            });
            continue;
        }
        for g in seg.text.graphemes(true) {
            let w = (UnicodeWidthStr::width(g) as u16).max(1);
            let is_space = g.chars().all(|c| c.is_whitespace()) && g != "\n";
            out.push(Atom {
                g: g.to_string(),
                w,
                seg: StyledSegment {
                    text: g.to_string(),
                    ..seg.clone()
                },
                is_space,
                is_newline: false,
            });
        }
    }
    out
}

fn wrap_segments(
    segments: &[StyledSegment],
    width: u16,
    first_prefix: &[StyledSegment],
    cont_prefix: &[StyledSegment],
) -> Vec<StyledLine> {
    let width = width.max(1);

    let mut atoms = Vec::new();
    atoms.extend(segments_to_atoms(first_prefix));
    let prefix_w = atoms.iter().map(|a| a.w).sum::<u16>();
    let content_atoms = segments_to_atoms(segments);

    let mut lines: Vec<Vec<Atom>> = Vec::new();
    let mut current: Vec<Atom> = atoms;
    let mut current_w = prefix_w.min(width);
    let mut is_first_line = true;

    let mut i = 0usize;
    while i < content_atoms.len() {
        let a = &content_atoms[i];
        if a.is_newline {
            // Trim trailing spaces.
            trim_trailing_spaces(&mut current);
            lines.push(std::mem::take(&mut current));
            current = segments_to_atoms(cont_prefix);
            current_w = current.iter().map(|x| x.w).sum::<u16>().min(width);
            is_first_line = false;
            i += 1;
            continue;
        }

        // Skip leading spaces at the start of a line (after prefix).
        let line_prefix_w = if is_first_line { prefix_w } else { cell_width_segments(cont_prefix) };
        if a.is_space && current_w == line_prefix_w {
            i += 1;
            continue;
        }

        if current_w.saturating_add(a.w) <= width {
            current.push(a.clone());
            current_w = current_w.saturating_add(a.w);
            i += 1;
            continue;
        }

        // Line full: break.
        trim_trailing_spaces(&mut current);
        lines.push(std::mem::take(&mut current));
        current = segments_to_atoms(cont_prefix);
        current_w = current.iter().map(|x| x.w).sum::<u16>().min(width);
        is_first_line = false;

        // If the atom is still too wide for an empty line, force-place it (it will occupy at least one cell).
        if current_w.saturating_add(a.w) > width && current_w < width {
            current.push(a.clone());
            current_w = current_w.saturating_add(a.w).min(width);
            i += 1;
        }
    }

    trim_trailing_spaces(&mut current);
    lines.push(current);

    lines.into_iter().map(atoms_to_line).collect()
}

fn atoms_to_line(atoms: Vec<Atom>) -> StyledLine {
    let mut segments: Vec<StyledSegment> = Vec::new();
    for atom in atoms {
        if matches!(atom.seg.kind, SegmentKind::LineBreak) {
            continue;
        }
        if let Some(last) = segments.last_mut()
            && last.kind == atom.seg.kind
            && last.block_context == atom.seg.block_context
            && last.inline == atom.seg.inline
            && last.is_link_url == atom.seg.is_link_url
            && link_eq(&last.link, &atom.seg.link)
        {
            last.text.push_str(&atom.g);
        } else {
            segments.push(atom.seg);
        }
    }
    StyledLine { segments }
}

fn link_eq(a: &Option<Arc<str>>, b: &Option<Arc<str>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b) || *a == *b,
        _ => false,
    }
}

fn trim_trailing_spaces(atoms: &mut Vec<Atom>) {
    while atoms.last().is_some_and(|a| a.is_space) {
        atoms.pop();
    }
}

fn draw_styled_line(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    scroll_x: u16,
    line: &StyledLine,
    styles: &MarkdownStyles,
) {
    if width == 0 {
        return;
    }

    let mut col: u16 = 0;
    let end = scroll_x.saturating_add(width);
    let mut wrote_any = false;

    for seg in &line.segments {
        let seg_w = cell_width(&seg.text);
        let seg_end = col.saturating_add(seg_w);

        if seg_end <= scroll_x {
            col = seg_end;
            continue;
        }
        if col >= end {
            break;
        }

        // Slice the segment by width.
        let start_in_seg = scroll_x.saturating_sub(col);
        let visible_w = end.saturating_sub(col.max(scroll_x));
        let slice = slice_by_width(&seg.text, start_in_seg, visible_w);

        if slice.is_empty() {
            col = seg_end;
            continue;
        }

        let style = styles.style_for_segment(seg);
        let draw_x = x.saturating_add(col.max(scroll_x).saturating_sub(scroll_x));
        buf.set_stringn(draw_x, y, &slice, (width as usize).saturating_sub(draw_x.saturating_sub(x) as usize), style);
        wrote_any = true;

        col = seg_end;
    }

    if !wrote_any {
        // Ensure the row doesn't smear (outer caller typically clears, but keep safe).
        for dx in 0..width {
            buf[(x.saturating_add(dx), y)]
                .set_symbol(" ")
                .set_style(styles.base);
        }
    }
}

fn hit_test_line_link(line: &StyledLine, x: u16) -> Option<Arc<str>> {
    let mut col: u16 = 0;
    for seg in &line.segments {
        let w = cell_width(&seg.text);
        let next = col.saturating_add(w);
        if x >= col && x < next {
            if let Some(url) = &seg.link {
                return Some(Arc::clone(url));
            }
            return None;
        }
        col = next;
    }
    None
}

fn fill_rect(buf: &mut Buffer, area: Rect, style: Style) {
    for dy in 0..area.height {
        let y = area.y.saturating_add(dy);
        for dx in 0..area.width {
            let x = area.x.saturating_add(dx);
            buf[(x, y)].set_symbol(" ").set_style(style);
        }
    }
}

fn cell_width(text: &str) -> u16 {
    let mut out: u16 = 0;
    for g in text.graphemes(true) {
        out = out.saturating_add((UnicodeWidthStr::width(g) as u16).max(1));
    }
    out
}

fn cell_width_segments(segs: &[StyledSegment]) -> u16 {
    segs.iter().map(|s| cell_width(&s.text)).sum::<u16>()
}

fn line_width_cells(line: &StyledLine) -> u16 {
    line.segments.iter().map(|s| cell_width(&s.text)).sum()
}

fn slice_by_width(text: &str, start_col: u16, width: u16) -> String {
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut col: u16 = 0;
    let end = start_col.saturating_add(width);

    for g in text.graphemes(true) {
        let w = (UnicodeWidthStr::width(g) as u16).max(1);
        let next = col.saturating_add(w);

        if next <= start_col {
            col = next;
            continue;
        }
        if col >= end {
            break;
        }

        out.push_str(g);
        col = next;
    }

    out
}

fn add_signed(v: u16, dv: i16) -> u16 {
    if dv >= 0 {
        v.saturating_add(dv as u16)
    } else {
        v.saturating_sub(dv.wrapping_abs() as u16)
    }
}

fn border_glyphs(is_top: bool) -> (&'static str, &'static str, &'static str) {
    // Use box drawing for borders; themes can override global window borders via glyphs, but this
    // keeps markdown blocks stable without requiring new glyph keys.
    if is_top {
        ("┌", "─", "┐")
    } else {
        ("└", "─", "┘")
    }
}

fn render_table_lines(header: &[Vec<StyledSegment>], rows: &[Vec<Vec<StyledSegment>>]) -> Vec<StyledLine> {
    // Determine column count.
    let mut cols = header.len();
    for r in rows {
        cols = cols.max(r.len());
    }
    if cols == 0 {
        return Vec::new();
    }

    // Compute column widths.
    let mut widths: Vec<u16> = vec![0; cols];
    for (idx, cell) in header.iter().enumerate() {
        widths[idx] = widths[idx].max(inlines_width(cell));
    }
    for r in rows {
        for (idx, cell) in r.iter().enumerate() {
            widths[idx] = widths[idx].max(inlines_width(cell));
        }
    }

    let mut lines = Vec::new();
    lines.push(table_border_line(&widths, '-'));
    lines.push(table_row_line(&widths, header, true));
    lines.push(table_border_line(&widths, '='));
    for r in rows {
        lines.push(table_row_line(&widths, r, false));
        lines.push(table_border_line(&widths, '-'));
    }
    lines
}

fn inlines_width(inlines: &[StyledSegment]) -> u16 {
    inlines.iter().map(|s| cell_width(&s.text)).sum()
}

fn table_border_line(widths: &[u16], ch: char) -> StyledLine {
    let mut segs = Vec::new();
    segs.push(
        StyledSegment::marker("+", MarkerKind::General).with_block_context(BlockContext::TableBorder),
    );
    for (i, w) in widths.iter().enumerate() {
        let dash = ch.to_string().repeat((*w).saturating_add(2) as usize);
        segs.push(
            StyledSegment::marker(dash, MarkerKind::General).with_block_context(BlockContext::TableBorder),
        );
        segs.push(
            StyledSegment::marker(if i + 1 == widths.len() { "+" } else { "+" }, MarkerKind::General)
                .with_block_context(BlockContext::TableBorder),
        );
    }
    StyledLine::new(segs)
}

fn table_row_line(widths: &[u16], row: &[Vec<StyledSegment>], is_header: bool) -> StyledLine {
    let mut segs = Vec::new();
    segs.push(
        StyledSegment::marker("|", MarkerKind::General).with_block_context(BlockContext::TableBorder),
    );
    for (idx, w) in widths.iter().enumerate() {
        let cell = row.get(idx).map(Vec::as_slice).unwrap_or(&[]);
        let ctx = if is_header {
            BlockContext::TableHeader
        } else {
            BlockContext::TableCell
        };

        // Leading padding.
        segs.push(StyledSegment::text(" ").with_block_context(ctx));

        // Cell content.
        let mut used = 0u16;
        for s in cell {
            let seg = s.clone().with_block_context(ctx);
            // Keep markers visible inside cells, but style them via the markdown marker styles.
            used = used.saturating_add(cell_width(&seg.text));
            segs.push(seg);
        }

        // Trailing padding to fill the column.
        if used < *w {
            segs.push(StyledSegment::text(" ".repeat((w - used) as usize)).with_block_context(ctx));
        }
        segs.push(StyledSegment::text(" ").with_block_context(ctx));

        segs.push(
            StyledSegment::marker("|", MarkerKind::General).with_block_context(BlockContext::TableBorder),
        );
    }
    StyledLine::new(segs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::MouseEventKind;
    use crossterm::event::{KeyModifiers, MouseEvent};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use ratatui::layout::Rect;
    use crate::view::{ScrollbarHost, TabMode, ViewContext};
    use crate::wm::WindowId;

    #[test]
    fn hit_test_link_finds_url_in_wrapped_output() {
        let md = "> A blockquote with a [link](https://example.com/docs).";
        let doc = parse_markdown_document(md);
        let layout = MarkdownLayout::build(&doc, 80, 6, 6);

        let needle = "https://example.com/docs";

        for block in &layout.blocks {
            let LayoutBlock::Text { y, lines } = block else {
                continue;
            };
            for (idx, line) in lines.iter().enumerate() {
                let rendered: String = line.segments.iter().map(|s| s.text.as_str()).collect();
                if let Some(byte_idx) = rendered.find(needle) {
                    let x = cell_width(&rendered[..byte_idx]);
                    let doc_y = y.saturating_add(idx as u16);

                    let hit = layout.hit_test_link(x, doc_y).expect("hit link");
                    assert_eq!(&*hit, needle);
                    return;
                }
            }
        }

        panic!("did not find {needle:?} in rendered lines");
    }

    #[test]
    fn code_block_scrolls_horizontally_on_scroll_right() {
        let md = r#"```rust
let very_long_line = "SCROLL_RIGHT_TO_SEE_END_ABCDEFGHIJKLMNOPQRSTUVWXYZ_CODE_END_98765";
println!("{very_long_line}");
```"#;

        let doc = parse_markdown_document(md);
        let mut layout = MarkdownLayout::build(&doc, 50, 6, 6);

        let (y, block) = layout
            .blocks
            .iter()
            .find_map(|b| match b {
                LayoutBlock::CodeBlock { y, block } => Some((*y, block)),
                _ => None,
            })
            .expect("code block");

        let (inner_w, inner_h) = block.inner_viewport(layout.viewport_width);
        assert_eq!(inner_h, block.viewport_height);
        assert!(
            block.content_width > inner_w,
            "expected code line wider than viewport"
        );

        let changed = layout
            .handle_wheel(0, y.saturating_add(2), MouseEventKind::ScrollRight)
            .expect("wheel handled");
        assert!(changed, "scroll right should change scroll_x");

        let (_, block_after) = layout
            .blocks
            .iter()
            .find_map(|b| match b {
                LayoutBlock::CodeBlock { y, block } => Some((*y, block)),
                _ => None,
            })
            .expect("code block");
        assert!(block_after.scroll_x > 0, "scroll_x should advance");
    }

    fn find_ascii_in_terminal(
        terminal: &Terminal<TestBackend>,
        needle: &str,
    ) -> Option<(u16, u16)> {
        let buf = terminal.backend().buffer();
        let width = buf.area.width;
        let height = buf.area.height;
        let chars: Vec<char> = needle.chars().collect();
        if chars.is_empty() {
            return None;
        }

        for y in 0..height {
            for x in 0..width {
                if x.saturating_add(chars.len() as u16) > width {
                    break;
                }
                let mut ok = true;
                for (i, ch) in chars.iter().enumerate() {
                    let cell = buf[(x.saturating_add(i as u16), y)].symbol();
                    if cell != ch.encode_utf8(&mut [0u8; 4]) {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    return Some((x, y));
                }
            }
        }
        None
    }

    #[test]
    fn markdown_viewer_code_block_scroll_right_updates_rendered_buffer() {
        const MARKDOWN: &str = r#"# Title

## Code

```rust
let very_long_line = "SCROLL_RIGHT_TO_SEE_END_ABCDEFGHIJKLMNOPQRSTUVWXYZ_CODE_END_98765";
println!("{very_long_line}");
```"#;

        let theme = Theme::dark();
        let mut viewer = MarkdownViewer::new_with_width(MARKDOWN, 72u16)
            .vertical_scrollbar(ScrollbarVisibility::Always)
            .code_block_max_height(6u16);

        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        let area = Rect::new(2, 2, 74, 16);
        let ctx = ViewContext {
            theme: &theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::View,
            tab_mode: TabMode::Cycle,
        };

        // Initial draw.
        terminal
            .draw(|f| viewer.draw(f, area, ctx))
            .unwrap();

        // Scroll down until the code fence is visible.
        for _ in 0..40 {
            if find_ascii_in_terminal(&terminal, "```rust").is_some() {
                break;
            }
            let ev = Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: area.x.saturating_add(1),
                row: area.y.saturating_add(1),
                modifiers: KeyModifiers::NONE,
            });
            let _ = viewer.handle_event(&ev, ctx);
            terminal
                .draw(|f| viewer.draw(f, area, ctx))
                .unwrap();
        }
        assert!(
            find_ascii_in_terminal(&terminal, "```rust").is_some(),
            "expected code block to be visible after scrolling"
        );

        let (col, row) =
            find_ascii_in_terminal(&terminal, "let very_long_line").expect("find code line");
        let before_line: String = {
            let buf = terminal.backend().buffer();
            let mut s = String::new();
            for x in 0..buf.area.width {
                s.push_str(buf[(x, row)].symbol());
            }
            s
        };

        // Scroll right within the code block.
        let mut any_consumed = false;
        for _ in 0..40 {
            if find_ascii_in_terminal(&terminal, "CODE_END_98765").is_some() {
                break;
            }
            let ev = Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollRight,
                column: col,
                row,
                modifiers: KeyModifiers::NONE,
            });
            let res = viewer.handle_event(&ev, ctx);
            any_consumed |= res.is_consumed();
            terminal
                .draw(|f| viewer.draw(f, area, ctx))
                .unwrap();
        }

        assert!(
            any_consumed,
            "expected at least one scroll-right event to be consumed"
        );

        let after_line: String = {
            let buf = terminal.backend().buffer();
            let mut s = String::new();
            for x in 0..buf.area.width {
                s.push_str(buf[(x, row)].symbol());
            }
            s
        };
        assert_ne!(
            before_line, after_line,
            "expected code line to change after horizontal scrolling"
        );

        assert!(
            find_ascii_in_terminal(&terminal, "CODE_END_98765").is_some(),
            "expected horizontal scroll to reveal end marker"
        );
    }

    #[test]
    fn markdown_viewer_demo_markdown_scrolls_code_and_table_horizontally() {
        const MARKDOWN: &str = r#"# Markdown Viewer

## Inline

This is **bold**, *italic*, and ~~strikethrough~~.

> A blockquote with a [link](https://example.com/docs).

## Lists

- Unordered item one
- Unordered item two
1. Ordered item one
2. Ordered item two

## Code

```rust
let very_long_line = "SCROLL_RIGHT_TO_SEE_END_ABCDEFGHIJKLMNOPQRSTUVWXYZ_CODE_END_98765";
println!("{very_long_line}");
```

## Table

| Column A | Column B |
|----------|----------|
| short    | value    |
| long     | TABLE_SCROLL_RIGHT_TO_SEE_END_ABCDEFGHIJKLMNOPQRSTUVWXYZ_TABLE_END_98765 |
"#;

        let theme = Theme::dark();
        let mut viewer = MarkdownViewer::new_with_width(MARKDOWN, 72u16)
            .vertical_scrollbar(ScrollbarVisibility::Always)
            .code_block_max_height(6u16)
            .table_max_height(7u16);

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let area = Rect::new(2, 2, 74, 19);
        let ctx = ViewContext {
            theme: &theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::View,
            tab_mode: TabMode::Cycle,
        };

        terminal
            .draw(|f| viewer.draw(f, area, ctx))
            .unwrap();

        // Scroll down until the code block is visible.
        for _ in 0..50 {
            if find_ascii_in_terminal(&terminal, "```rust").is_some() {
                break;
            }
            let ev = Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: area.x.saturating_add(1),
                row: area.y.saturating_add(1),
                modifiers: KeyModifiers::NONE,
            });
            let _ = viewer.handle_event(&ev, ctx);
            terminal
                .draw(|f| viewer.draw(f, area, ctx))
                .unwrap();
        }
        assert!(
            find_ascii_in_terminal(&terminal, "```rust").is_some(),
            "expected code block to be visible"
        );

        let (col, row) =
            find_ascii_in_terminal(&terminal, "let very_long_line").expect("find code line");

        for _ in 0..40 {
            if find_ascii_in_terminal(&terminal, "CODE_END_98765").is_some() {
                break;
            }
            let ev = Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollRight,
                column: col,
                row,
                modifiers: KeyModifiers::NONE,
            });
            let _ = viewer.handle_event(&ev, ctx);
            terminal
                .draw(|f| viewer.draw(f, area, ctx))
                .unwrap();
        }

        assert!(
            find_ascii_in_terminal(&terminal, "CODE_END_98765").is_some(),
            "expected CODE_END marker to become visible after scroll-right"
        );

        // Now scroll down to the table and ensure the long row is visible.
        for _ in 0..80 {
            if find_ascii_in_terminal(&terminal, "TABLE_SCROLL_RIGHT_TO_SEE_END").is_some() {
                break;
            }
            let ev = Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: area.x.saturating_add(1),
                row: area.y.saturating_add(1),
                modifiers: KeyModifiers::NONE,
            });
            let _ = viewer.handle_event(&ev, ctx);
            terminal
                .draw(|f| viewer.draw(f, area, ctx))
                .unwrap();
        }
        assert!(
            find_ascii_in_terminal(&terminal, "TABLE_SCROLL_RIGHT_TO_SEE_END").is_some(),
            "expected table row to be visible"
        );

        let (tcol, trow) = find_ascii_in_terminal(&terminal, "TABLE_SCROLL_RIGHT_TO_SEE_END")
            .expect("find table row");
        for _ in 0..60 {
            if find_ascii_in_terminal(&terminal, "TABLE_END_98765").is_some() {
                break;
            }
            let ev = Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollRight,
                column: tcol,
                row: trow,
                modifiers: KeyModifiers::NONE,
            });
            let _ = viewer.handle_event(&ev, ctx);
            terminal
                .draw(|f| viewer.draw(f, area, ctx))
                .unwrap();
        }
        assert!(
            find_ascii_in_terminal(&terminal, "TABLE_END_98765").is_some(),
            "expected TABLE_END marker to become visible after scroll-right"
        );
    }
}
