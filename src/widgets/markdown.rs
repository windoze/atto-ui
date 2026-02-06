use std::cmp;
use std::sync::Arc;

use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use parking_lot::RwLock;
use pulldown_cmark::{CodeBlockKind, Event as MdEvent, Options, Parser, Tag, TagEnd};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Block;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::composable::scroll::{
    ScrollbarDrag, ScrollbarHit, scroll_offset_from_thumb_start, scrollbar_hit_test,
    scrollbar_layout_1d, should_show_scrollbar,
};
use crate::composable::{
    Component, ComponentContext, EventResult, ScrollConfig, ScrollContainer, ScrollContainerHost,
    ScrollContent, ScrollContentContext, ScrollOffset, ScrollbarVisibility,
};
use crate::reactive::{Binding, DirtyObserver};
use crate::theme::Theme;

const DEFAULT_CODE_BLOCK_MAX_HEIGHT: u16 = 8;
const DEFAULT_TABLE_MAX_HEIGHT: u16 = 8;
const DEFAULT_SCROLL_STEP: u16 = 3;
const LIST_INDENT_SPACES: usize = 2;

type LinkCallBackType = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone)]
struct LinkCallback(Arc<RwLock<Option<LinkCallBackType>>>);

impl LinkCallback {
    fn new() -> Self {
        Self(Arc::new(RwLock::new(None)))
    }

    fn set(&self, cb: Option<LinkCallBackType>) {
        *self.0.write() = cb;
    }

    fn fire(&self, url: &str) {
        if let Some(cb) = &*self.0.read() {
            cb(url);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmbeddedScrollbarTarget {
    Code(usize),
    Table(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EmbeddedScrollbarDragState {
    target: EmbeddedScrollbarTarget,
    drag: ScrollbarDrag,
}

/// Markdown viewer component.
pub struct MarkdownViewer {
    shared: Arc<RwLock<MarkdownShared>>,
    scroll_config: Binding<ScrollConfig>,
    scroll: ScrollContainer,
}

impl MarkdownViewer {
    pub fn new(markdown: impl Into<Binding<String>>) -> Self {
        let markdown = markdown.into();
        let width = Binding::new(None);
        let show_markers = Binding::new(false);
        let vertical_scrollbar = Binding::new(ScrollbarVisibility::Auto);
        let max_code_height = Binding::new(DEFAULT_CODE_BLOCK_MAX_HEIGHT);
        let max_table_height = Binding::new(DEFAULT_TABLE_MAX_HEIGHT);
        let fg_override = Binding::new(None);
        let bg_override = Binding::new(None);
        let link_callback = LinkCallback::new();

        let shared = Arc::new(RwLock::new(MarkdownShared::new(
            markdown.clone(),
            width.clone(),
            show_markers.clone(),
            vertical_scrollbar.clone(),
            max_code_height.clone(),
            max_table_height.clone(),
            fg_override.clone(),
            bg_override.clone(),
            link_callback.clone(),
        )));

        let scroll_config = Binding::new(
            ScrollConfig::default()
                .vertical_scrollbar(vertical_scrollbar.get())
                .horizontal_scrollbar(ScrollbarVisibility::Never),
        );
        let content = MarkdownContent {
            shared: shared.clone(),
        };
        let scroll =
            ScrollContainer::new(Box::new(content)).with_scroll_config(scroll_config.clone());

        Self {
            shared,
            scroll_config,
            scroll,
        }
    }

    pub fn markdown(self, markdown: impl Into<String>) -> Self {
        self.shared.write().markdown.set(markdown.into());
        self
    }

    pub fn wrap_width(self, width: u16) -> Self {
        self.shared.write().width.set(Some(width));
        self
    }

    pub fn width(self, width: u16) -> Self {
        self.wrap_width(width)
    }

    pub fn show_markers(self, show: bool) -> Self {
        self.shared.write().show_markers.set(show);
        self
    }

    pub fn vertical_scrollbar(self, vis: ScrollbarVisibility) -> Self {
        self.shared.write().vertical_scrollbar.set(vis);
        self.scroll_config.update(|cfg| {
            cfg.vertical_scrollbar = vis;
        });
        self
    }

    pub fn code_block_max_height(self, height: u16) -> Self {
        self.shared.write().max_code_height.set(height);
        self
    }

    pub fn table_max_height(self, height: u16) -> Self {
        self.shared.write().max_table_height.set(height);
        self
    }

    pub fn text_color(self, color: Color) -> Self {
        self.shared.write().fg_override.set(Some(color));
        self
    }

    pub fn background(self, color: Color) -> Self {
        self.shared.write().bg_override.set(Some(color));
        self
    }

    pub fn on_link<F>(self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.shared
            .write()
            .link_callback
            .set(Some(Arc::new(callback)));
        self
    }
}

impl Component for MarkdownViewer {
    fn is_focusable(&self) -> bool {
        self.scroll.is_focusable()
    }

    fn focus_first(&mut self) -> bool {
        self.scroll.focus_first()
    }

    fn focus_last(&mut self) -> bool {
        self.scroll.focus_last()
    }

    fn min_width(&self) -> u16 {
        self.scroll.min_width()
    }

    fn min_height(&self) -> u16 {
        self.scroll.min_height()
    }

    fn desired_width(&self) -> Option<u16> {
        self.scroll.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.scroll.desired_height()
    }

    fn is_scrollable(&self) -> bool {
        self.scroll.is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        self.scroll.content_size()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.scroll.viewport_size()
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.scroll.scroll_config()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.scroll.scroll_offset()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.scroll.set_scroll_offset(x, y);
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.scroll.handle_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.scroll.draw(frame, area, ctx);
    }
}

struct MarkdownContent {
    shared: Arc<RwLock<MarkdownShared>>,
}

impl ScrollContent for MarkdownContent {
    fn desired_width(&self) -> Option<u16> {
        self.shared.read().width.get()
    }

    fn desired_height(&self) -> Option<u16> {
        let mut shared = self.shared.write();
        if shared.vertical_scrollbar.get() != ScrollbarVisibility::Never {
            return None;
        }

        let wrap_width = shared.width.get().or(shared.last_wrap_width).unwrap_or(0);
        if wrap_width == 0 {
            return None;
        }
        let layout_width = wrap_width.max(1);
        shared.ensure_layout(layout_width);
        shared.layout.as_ref().map(|layout| layout.total_height)
    }

    fn content_size(&mut self, viewport: (u16, u16), _ctx: ScrollContentContext<'_>) -> (u16, u16) {
        let mut shared = self.shared.write();
        let wrap_width = shared.resolve_wrap_width(viewport.0);
        let layout_width = wrap_width.max(1);
        shared.ensure_layout(layout_width);
        let height = shared
            .layout
            .as_ref()
            .map(|layout| layout.total_height)
            .unwrap_or(0);
        (wrap_width, height)
    }

    fn handle_event(
        &mut self,
        event: &Event,
        ctx: ScrollContentContext<'_>,
        _host: &mut ScrollContainerHost,
    ) -> EventResult {
        let mut shared = self.shared.write();
        let layout = shared.layout.clone();
        let Some(layout) = layout else {
            return EventResult::ignored();
        };
        let scroll = ctx.info.scroll_offset;
        let viewport = ctx.info.viewport_size;
        if viewport.0 == 0 || viewport.1 == 0 {
            return EventResult::ignored();
        }

        let Event::Mouse(m) = event else {
            return EventResult::ignored();
        };

        if shared.embedded_scrollbar_drag.is_some()
            && let Some(res) = shared.handle_embedded_scrollbar_drag(*m, scroll, viewport, &layout)
        {
            return res;
        }

        let content_x = scroll.x.saturating_add(m.column);
        let content_y = scroll.y.saturating_add(m.row);

        if let Some(block_idx) = layout.block_at_row(content_y) {
            let block = &layout.blocks[block_idx];
            let local_y = content_y.saturating_sub(block.y);
            let local_x = content_x;
            if let Some(res) =
                shared.handle_block_event(block, local_x, local_y, *m, viewport, layout.wrap_width)
                && res.is_consumed()
            {
                return res;
            }
        }

        if let MouseEventKind::Down(MouseButton::Left) = m.kind
            && let Some(hit) = layout.link_at(content_x, content_y)
        {
            shared.link_callback.fire(&hit.url);
            return EventResult::consumed();
        }

        EventResult::ignored()
    }

    fn draw(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ScrollContentContext<'_>,
        _host: &mut ScrollContainerHost,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let mut shared = self.shared.write();
        let wrap_width = shared.resolve_wrap_width(area.width);
        let layout_width = wrap_width.max(1);
        shared.ensure_layout(layout_width);
        let layout = shared.layout.clone();
        let Some(layout) = layout else {
            return;
        };

        let styles = MarkdownStyles::resolve(ctx.component.theme, &shared);
        frame.render_widget(Block::default().style(styles.base), area);

        let scroll = ctx.info.scroll_offset;
        let viewport_h = area.height;
        let viewport_w = area.width;
        let content_width = layout.wrap_width.min(viewport_w);
        if viewport_h == 0 || viewport_w == 0 {
            return;
        }

        for block in layout.blocks.iter() {
            if block.y >= scroll.y.saturating_add(viewport_h) {
                break;
            }
            if block.y.saturating_add(block.height) <= scroll.y {
                continue;
            }

            match &block.kind {
                LayoutBlockKind::Text { lines, style } => {
                    let block_start = block.y;
                    let mut line_idx: u16 = 0;
                    for line in lines.iter() {
                        let content_y = block_start.saturating_add(line_idx);
                        line_idx = line_idx.saturating_add(1);
                        if content_y < scroll.y {
                            continue;
                        }
                        if content_y >= scroll.y.saturating_add(viewport_h) {
                            break;
                        }
                        let y = area.y.saturating_add(content_y.saturating_sub(scroll.y));
                        draw_line(frame, area.x, y, content_width, line, style, &styles);
                    }
                }
                LayoutBlockKind::Code {
                    index,
                    prefix,
                    in_blockquote,
                } => {
                    if let Some(code) = shared.code_blocks.get_mut(*index) {
                        draw_code_block(
                            frame,
                            area,
                            block,
                            code,
                            prefix,
                            scroll,
                            content_width,
                            &styles,
                            ctx.component.theme,
                            *in_blockquote,
                        );
                    }
                }
                LayoutBlockKind::Table {
                    index,
                    prefix,
                    in_blockquote,
                } => {
                    if let Some(table) = shared.tables.get_mut(*index) {
                        draw_table_block(
                            frame,
                            area,
                            block,
                            table,
                            prefix,
                            scroll,
                            content_width,
                            &styles,
                            ctx.component.theme,
                            *in_blockquote,
                        );
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
struct MarkdownShared {
    markdown: Binding<String>,
    width: Binding<Option<u16>>,
    show_markers: Binding<bool>,
    vertical_scrollbar: Binding<ScrollbarVisibility>,
    max_code_height: Binding<u16>,
    max_table_height: Binding<u16>,
    fg_override: Binding<Option<Color>>,
    bg_override: Binding<Option<Color>>,
    link_callback: LinkCallback,

    md_dirty: DirtyObserver,
    markers_dirty: DirtyObserver,
    max_code_dirty: DirtyObserver,
    max_table_dirty: DirtyObserver,

    parsed: Vec<MdBlock>,
    code_blocks: Vec<CodeBlockState>,
    tables: Vec<TableBlockState>,
    layout: Option<Layout>,
    last_wrap_width: Option<u16>,

    embedded_scrollbar_drag: Option<EmbeddedScrollbarDragState>,
}

impl MarkdownShared {
    #[allow(clippy::too_many_arguments)]
    fn new(
        markdown: Binding<String>,
        width: Binding<Option<u16>>,
        show_markers: Binding<bool>,
        vertical_scrollbar: Binding<ScrollbarVisibility>,
        max_code_height: Binding<u16>,
        max_table_height: Binding<u16>,
        fg_override: Binding<Option<Color>>,
        bg_override: Binding<Option<Color>>,
        link_callback: LinkCallback,
    ) -> Self {
        Self {
            md_dirty: markdown.dirty_observer(),
            markers_dirty: show_markers.dirty_observer(),
            max_code_dirty: max_code_height.dirty_observer(),
            max_table_dirty: max_table_height.dirty_observer(),
            markdown,
            width,
            show_markers,
            vertical_scrollbar,
            max_code_height,
            max_table_height,
            fg_override,
            bg_override,
            link_callback,
            parsed: Vec::new(),
            code_blocks: Vec::new(),
            tables: Vec::new(),
            layout: None,
            last_wrap_width: None,
            embedded_scrollbar_drag: None,
        }
    }

    fn handle_embedded_scrollbar_drag(
        &mut self,
        m: MouseEvent,
        scroll: ScrollOffset,
        viewport: (u16, u16),
        layout: &Layout,
    ) -> Option<EventResult> {
        let drag = self.embedded_scrollbar_drag?;
        match m.kind {
            MouseEventKind::Drag(MouseButton::Left) => {
                let content_x = scroll.x.saturating_add(m.column);
                let content_y = scroll.y.saturating_add(m.row);

                let Some(block) =
                    layout
                        .blocks
                        .iter()
                        .find(|block| match (&block.kind, drag.target) {
                            (
                                LayoutBlockKind::Code { index, .. },
                                EmbeddedScrollbarTarget::Code(id),
                            ) => *index == id,
                            (
                                LayoutBlockKind::Table { index, .. },
                                EmbeddedScrollbarTarget::Table(id),
                            ) => *index == id,
                            _ => false,
                        })
                else {
                    self.embedded_scrollbar_drag = None;
                    return Some(EventResult::consumed());
                };

                let local_x = content_x;
                let local_y = content_y.saturating_sub(block.y);

                let (target_content_w, target_content_h, target_scroll) = match drag.target {
                    EmbeddedScrollbarTarget::Code(id) => {
                        let Some(code) = self.code_blocks.get(id) else {
                            self.embedded_scrollbar_drag = None;
                            return Some(EventResult::consumed());
                        };
                        let (w, h) = code.content_size();
                        (w, h, code.scroll)
                    }
                    EmbeddedScrollbarTarget::Table(id) => {
                        let Some(table) = self.tables.get(id) else {
                            self.embedded_scrollbar_drag = None;
                            return Some(EventResult::consumed());
                        };
                        let (w, h) = table.content_size();
                        (w, h, table.scroll)
                    }
                };

                let (target_scroll, embedded) = solve_embedded_scroll_and_layout(
                    target_scroll,
                    (target_content_w, target_content_h),
                    block,
                    viewport,
                    layout.wrap_width,
                );

                let prefix_width = match &block.kind {
                    LayoutBlockKind::Code { prefix, .. }
                    | LayoutBlockKind::Table { prefix, .. } => {
                        prefix.first_width.max(prefix.rest_width)
                    }
                    _ => 0,
                };

                let scroll_after_drag = apply_embedded_scrollbar_drag(
                    target_scroll,
                    (target_content_w, target_content_h),
                    embedded,
                    prefix_width,
                    local_x,
                    local_y,
                    drag.drag,
                );

                match drag.target {
                    EmbeddedScrollbarTarget::Code(id) => {
                        let Some(code) = self.code_blocks.get_mut(id) else {
                            self.embedded_scrollbar_drag = None;
                            return Some(EventResult::consumed());
                        };
                        code.scroll = scroll_after_drag;
                    }
                    EmbeddedScrollbarTarget::Table(id) => {
                        let Some(table) = self.tables.get_mut(id) else {
                            self.embedded_scrollbar_drag = None;
                            return Some(EventResult::consumed());
                        };
                        table.scroll = scroll_after_drag;
                    }
                }

                // If scrollbars disappeared mid-drag (e.g. viewport shrank), stop the drag.
                let should_keep_drag = match drag.drag {
                    ScrollbarDrag::Vertical { .. } => embedded.show_v,
                    ScrollbarDrag::Horizontal { .. } => embedded.show_h,
                };
                if !should_keep_drag {
                    self.embedded_scrollbar_drag = None;
                }

                Some(EventResult::consumed())
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.embedded_scrollbar_drag = None;
                Some(EventResult::consumed())
            }
            _ => None,
        }
    }

    fn resolve_wrap_width(&self, viewport_width: u16) -> u16 {
        match self.width.get() {
            Some(w) => w.min(viewport_width),
            None => viewport_width,
        }
    }

    fn ensure_layout(&mut self, wrap_width: u16) {
        let markdown_changed = self.markdown.check_dirty(&mut self.md_dirty);
        let markers_changed = self.show_markers.check_dirty(&mut self.markers_dirty);
        let code_height_changed = self.max_code_height.check_dirty(&mut self.max_code_dirty);
        let table_height_changed = self.max_table_height.check_dirty(&mut self.max_table_dirty);
        let width_changed = self.last_wrap_width != Some(wrap_width);

        if markdown_changed || markers_changed || self.layout.is_none() {
            let show_markers = self.show_markers.get();
            self.parsed = parse_markdown(&self.markdown.get(), show_markers);
            let (codes, tables) = build_block_states(&self.parsed);
            self.code_blocks = codes;
            self.tables = tables;
        }

        if markdown_changed
            || markers_changed
            || code_height_changed
            || table_height_changed
            || width_changed
            || self.layout.is_none()
        {
            let max_code_height = self.max_code_height.get();
            let max_table_height = self.max_table_height.get();
            let show_markers = self.show_markers.get();
            let layout = build_layout(
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

    fn handle_block_event(
        &mut self,
        block: &LayoutBlock,
        local_x: u16,
        local_y: u16,
        m: MouseEvent,
        viewport: (u16, u16),
        wrap_width: u16,
    ) -> Option<EventResult> {
        let is_wheel = matches!(
            m.kind,
            MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
                | MouseEventKind::ScrollLeft
                | MouseEventKind::ScrollRight
        );

        match &block.kind {
            LayoutBlockKind::Code { index, prefix, .. } => {
                let prefix_width = prefix.first_width.max(prefix.rest_width);
                let total_width = wrap_width.min(viewport.0);
                if total_width == 0 || local_x >= total_width {
                    return None;
                }
                let outer_w = total_width.saturating_sub(prefix_width);
                if outer_w == 0 {
                    return None;
                }

                let code = self.code_blocks.get_mut(*index)?;
                let (content_w, content_h) = code.content_size();
                let embedded =
                    EmbeddedScrollView::solve_auto((content_w, content_h), (outer_w, block.height));

                // Click/drag on embedded scrollbars.
                if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                    if let Some(res) = handle_embedded_scrollbar_mouse_down(
                        &mut self.embedded_scrollbar_drag,
                        EmbeddedScrollbarTarget::Code(*index),
                        code.scroll,
                        (content_w, content_h),
                        embedded,
                        local_x,
                        local_y,
                        prefix_width,
                    ) {
                        code.scroll = res;
                        return Some(EventResult::consumed());
                    }
                }

                if is_wheel {
                    let embedded = EmbeddedScrollView::solve_auto(
                        (content_w, content_h),
                        (outer_w, block.height),
                    );
                    let consumed = code.handle_scroll(
                        m,
                        embedded.viewport_w,
                        embedded.viewport_h,
                        DEFAULT_SCROLL_STEP,
                    );
                    if consumed {
                        return Some(EventResult::consumed());
                    }
                    return None;
                }
                None
            }
            LayoutBlockKind::Table { index, prefix, .. } => {
                let prefix_width = prefix.first_width.max(prefix.rest_width);
                let total_width = wrap_width.min(viewport.0);
                if total_width == 0 || local_x >= total_width {
                    return None;
                }
                let outer_w = total_width.saturating_sub(prefix_width);
                if outer_w == 0 {
                    return None;
                }

                let table = self.tables.get_mut(*index)?;
                let (content_w, content_h) = table.content_size();
                let embedded =
                    EmbeddedScrollView::solve_auto((content_w, content_h), (outer_w, block.height));

                // Click/drag on embedded scrollbars.
                if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                    if let Some(res) = handle_embedded_scrollbar_mouse_down(
                        &mut self.embedded_scrollbar_drag,
                        EmbeddedScrollbarTarget::Table(*index),
                        table.scroll,
                        (content_w, content_h),
                        embedded,
                        local_x,
                        local_y,
                        prefix_width,
                    ) {
                        table.scroll = res;
                        return Some(EventResult::consumed());
                    }
                }

                if is_wheel {
                    let embedded = EmbeddedScrollView::solve_auto(
                        (content_w, content_h),
                        (outer_w, block.height),
                    );
                    let consumed = table.handle_scroll(
                        m,
                        embedded.viewport_w,
                        embedded.viewport_h,
                        DEFAULT_SCROLL_STEP,
                    );
                    if consumed {
                        return Some(EventResult::consumed());
                    }
                    return None;
                }

                let content_x = local_x.saturating_sub(prefix_width);

                let max_x = content_w.saturating_sub(embedded.viewport_w);
                let max_y = content_h.saturating_sub(embedded.viewport_h);
                table.scroll.x = table.scroll.x.min(max_x);
                table.scroll.y = table.scroll.y.min(max_y);

                if local_x >= prefix_width
                    && content_x < embedded.viewport_w
                    && local_y < embedded.viewport_h
                {
                    if let MouseEventKind::Down(MouseButton::Left) = m.kind
                        && let Some(url) = table.link_at(content_x, local_y)
                    {
                        self.link_callback.fire(&url);
                        return Some(EventResult::consumed());
                    }
                }
                None
            }
            _ => None,
        }
    }
}

fn solve_embedded_scroll_and_layout(
    scroll: ScrollOffset,
    content: (u16, u16),
    block: &LayoutBlock,
    viewport: (u16, u16),
    wrap_width: u16,
) -> (ScrollOffset, EmbeddedScrollView) {
    let prefix_width = match &block.kind {
        LayoutBlockKind::Code { prefix, .. } | LayoutBlockKind::Table { prefix, .. } => {
            prefix.first_width.max(prefix.rest_width)
        }
        _ => 0,
    };

    let total_width = wrap_width.min(viewport.0);
    let outer_w = total_width.saturating_sub(prefix_width);
    let outer_h = block.height;

    let embedded = EmbeddedScrollView::solve_auto(content, (outer_w, outer_h));
    let max_x = content.0.saturating_sub(embedded.viewport_w);
    let max_y = content.1.saturating_sub(embedded.viewport_h);
    (
        ScrollOffset {
            x: scroll.x.min(max_x),
            y: scroll.y.min(max_y),
        },
        embedded,
    )
}

fn apply_embedded_scrollbar_drag(
    scroll: ScrollOffset,
    content: (u16, u16),
    embedded: EmbeddedScrollView,
    prefix_width: u16,
    local_x: u16,
    local_y: u16,
    drag: ScrollbarDrag,
) -> ScrollOffset {
    let mut scroll = scroll;

    match drag {
        ScrollbarDrag::Vertical { grab_offset } => {
            if !embedded.show_v || embedded.viewport_h == 0 {
                return scroll;
            }
            let bar_len = embedded.viewport_h;
            let layout =
                scrollbar_layout_1d(bar_len, embedded.viewport_h, content.1, scroll.y, true);
            if layout.track_len == 0 {
                return scroll;
            }

            let pos = local_y.min(bar_len.saturating_sub(1));
            let pos_in_track = pos
                .saturating_sub(layout.track_start)
                .min(layout.track_len.saturating_sub(1));
            let max_start = layout.track_len.saturating_sub(layout.thumb_len);
            let new_thumb_start = pos_in_track.saturating_sub(grab_offset).min(max_start);
            let new_y = scroll_offset_from_thumb_start(
                layout.track_len,
                embedded.viewport_h,
                content.1,
                new_thumb_start,
            );
            scroll.y = new_y;
        }
        ScrollbarDrag::Horizontal { grab_offset } => {
            if !embedded.show_h || embedded.viewport_w == 0 {
                return scroll;
            }
            let bar_len = embedded.viewport_w;
            let layout =
                scrollbar_layout_1d(bar_len, embedded.viewport_w, content.0, scroll.x, true);
            if layout.track_len == 0 {
                return scroll;
            }

            let local_x_in_bar = local_x.saturating_sub(prefix_width);
            let pos = local_x_in_bar.min(bar_len.saturating_sub(1));
            let pos_in_track = pos
                .saturating_sub(layout.track_start)
                .min(layout.track_len.saturating_sub(1));
            let max_start = layout.track_len.saturating_sub(layout.thumb_len);
            let new_thumb_start = pos_in_track.saturating_sub(grab_offset).min(max_start);
            let new_x = scroll_offset_from_thumb_start(
                layout.track_len,
                embedded.viewport_w,
                content.0,
                new_thumb_start,
            );
            scroll.x = new_x;
        }
    }

    let max_x = content.0.saturating_sub(embedded.viewport_w);
    let max_y = content.1.saturating_sub(embedded.viewport_h);
    scroll.x = scroll.x.min(max_x);
    scroll.y = scroll.y.min(max_y);

    scroll
}

fn handle_embedded_scrollbar_mouse_down(
    drag_state: &mut Option<EmbeddedScrollbarDragState>,
    target: EmbeddedScrollbarTarget,
    scroll: ScrollOffset,
    content: (u16, u16),
    embedded: EmbeddedScrollView,
    local_x: u16,
    local_y: u16,
    prefix_width: u16,
) -> Option<ScrollOffset> {
    let mut scroll = scroll;

    let bar_x_v = prefix_width.saturating_add(embedded.viewport_w);
    let bar_y_h = embedded.viewport_h;
    let arrows = true;

    // Vertical scrollbar hit-test.
    if embedded.show_v
        && local_x == bar_x_v
        && embedded.viewport_h > 0
        && local_y < embedded.viewport_h
    {
        let layout = scrollbar_layout_1d(
            embedded.viewport_h,
            embedded.viewport_h,
            content.1,
            scroll.y,
            arrows,
        );
        let pos = local_y.min(layout.bar_len.saturating_sub(1));
        match scrollbar_hit_test(layout, pos) {
            ScrollbarHit::ArrowDec => scroll.y = scroll.y.saturating_sub(1),
            ScrollbarHit::ArrowInc => {
                let max = content.1.saturating_sub(embedded.viewport_h);
                scroll.y = scroll.y.saturating_add(1).min(max);
            }
            ScrollbarHit::TrackDec => {
                let page = embedded.viewport_h;
                scroll.y = scroll.y.saturating_sub(page);
            }
            ScrollbarHit::TrackInc => {
                let max = content.1.saturating_sub(embedded.viewport_h);
                let page = embedded.viewport_h;
                scroll.y = scroll.y.saturating_add(page).min(max);
            }
            ScrollbarHit::Thumb { grab_offset } => {
                *drag_state = Some(EmbeddedScrollbarDragState {
                    target,
                    drag: ScrollbarDrag::Vertical { grab_offset },
                });
            }
            ScrollbarHit::None => {}
        }
        return Some(scroll);
    }

    // Horizontal scrollbar hit-test (bottom row of the embedded viewport).
    if embedded.show_h
        && embedded.viewport_w > 0
        && local_y == bar_y_h
        && local_x >= prefix_width
        && local_x < prefix_width.saturating_add(embedded.viewport_w)
    {
        let layout = scrollbar_layout_1d(
            embedded.viewport_w,
            embedded.viewport_w,
            content.0,
            scroll.x,
            arrows,
        );
        let local_x_in_bar = local_x.saturating_sub(prefix_width);
        let pos = local_x_in_bar.min(layout.bar_len.saturating_sub(1));
        match scrollbar_hit_test(layout, pos) {
            ScrollbarHit::ArrowDec => scroll.x = scroll.x.saturating_sub(1),
            ScrollbarHit::ArrowInc => {
                let max = content.0.saturating_sub(embedded.viewport_w);
                scroll.x = scroll.x.saturating_add(1).min(max);
            }
            ScrollbarHit::TrackDec => {
                let page = embedded.viewport_w;
                scroll.x = scroll.x.saturating_sub(page);
            }
            ScrollbarHit::TrackInc => {
                let max = content.0.saturating_sub(embedded.viewport_w);
                let page = embedded.viewport_w;
                scroll.x = scroll.x.saturating_add(page).min(max);
            }
            ScrollbarHit::Thumb { grab_offset } => {
                *drag_state = Some(EmbeddedScrollbarDragState {
                    target,
                    drag: ScrollbarDrag::Horizontal { grab_offset },
                });
            }
            ScrollbarHit::None => {}
        }
        return Some(scroll);
    }

    None
}

#[derive(Clone, Debug)]
enum MdBlock {
    Paragraph(Vec<InlineSpan>),
    Heading {
        level: u8,
        spans: Vec<InlineSpan>,
    },
    CodeBlock {
        id: usize,
        info: Option<String>,
        text: String,
    },
    Table {
        id: usize,
        headers: Vec<Vec<InlineSpan>>,
        rows: Vec<Vec<Vec<InlineSpan>>>,
    },
    List {
        ordered: bool,
        start: u64,
        items: Vec<ListItem>,
    },
    BlockQuote(Vec<MdBlock>),
}

#[derive(Clone, Debug)]
struct ListItem {
    blocks: Vec<MdBlock>,
}

#[derive(Clone, Debug)]
struct InlineSpan {
    text: String,
    inline: InlineStyle,
    link: Option<String>,
    kind: SpanKind,
}

#[derive(Clone, Copy, Debug, Default)]
struct InlineStyle {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpanKind {
    Text,
    Marker,
    Bullet,
}

#[derive(Clone, Debug)]
struct LineLayout {
    spans: Vec<InlineSpan>,
    width: u16,
}

#[derive(Clone, Debug)]
struct Layout {
    wrap_width: u16,
    blocks: Vec<LayoutBlock>,
    total_height: u16,
    link_hits: Vec<LinkHit>,
}

impl Layout {
    fn block_at_row(&self, row: u16) -> Option<usize> {
        self.blocks
            .iter()
            .position(|block| row >= block.y && row < block.y.saturating_add(block.height))
    }

    fn link_at(&self, col: u16, row: u16) -> Option<&LinkHit> {
        self.link_hits
            .iter()
            .find(|hit| hit.row == row && col >= hit.start && col < hit.end)
    }
}

#[derive(Clone, Debug)]
struct LayoutBlock {
    y: u16,
    height: u16,
    kind: LayoutBlockKind,
}

#[derive(Clone, Debug)]
enum LayoutBlockKind {
    Text {
        lines: Vec<LineLayout>,
        style: TextBlockStyle,
    },
    Code {
        index: usize,
        prefix: PrefixSpec,
        in_blockquote: bool,
    },
    Table {
        index: usize,
        prefix: PrefixSpec,
        in_blockquote: bool,
    },
}

#[derive(Clone, Debug)]
struct TextBlockStyle {
    kind: TextKind,
    in_blockquote: bool,
}

#[derive(Clone, Copy, Debug)]
enum TextKind {
    Paragraph,
    Heading(u8),
}

#[derive(Clone, Debug)]
struct PrefixSpec {
    first: Vec<InlineSpan>,
    rest: Vec<InlineSpan>,
    first_width: u16,
    rest_width: u16,
}

#[derive(Clone, Debug)]
struct LinkHit {
    row: u16,
    start: u16,
    end: u16,
    url: String,
}

#[derive(Clone, Debug)]
struct CodeBlockState {
    lines: Vec<String>,
    max_width: u16,
    scroll: ScrollOffset,
}

impl CodeBlockState {
    fn new(text: &str) -> Self {
        let mut lines: Vec<String> = text.split('\n').map(normalize_tabs).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        let max_width = lines.iter().map(|s| text_width(s)).max().unwrap_or(0);
        Self {
            lines,
            max_width,
            scroll: ScrollOffset::ZERO,
        }
    }

    fn content_size(&self) -> (u16, u16) {
        (
            self.max_width,
            self.lines.len().min(u16::MAX as usize) as u16,
        )
    }

    fn handle_scroll(
        &mut self,
        m: MouseEvent,
        viewport_w: u16,
        viewport_h: u16,
        step: u16,
    ) -> bool {
        let (content_w, content_h) = self.content_size();
        let mut scroll = self.scroll;
        let mut changed = false;

        let kind = normalize_wheel_kind(m.kind, m.modifiers);
        match kind {
            MouseEventKind::ScrollUp => {
                let dy = step as i16;
                let new_y = scroll.y.saturating_sub(dy as u16);
                if new_y != scroll.y {
                    scroll.y = new_y;
                    changed = true;
                }
            }
            MouseEventKind::ScrollDown => {
                let max = content_h.saturating_sub(viewport_h);
                let new_y = scroll.y.saturating_add(step).min(max);
                if new_y != scroll.y {
                    scroll.y = new_y;
                    changed = true;
                }
            }
            MouseEventKind::ScrollLeft => {
                let dx = step as i16;
                let new_x = scroll.x.saturating_sub(dx as u16);
                if new_x != scroll.x {
                    scroll.x = new_x;
                    changed = true;
                }
            }
            MouseEventKind::ScrollRight => {
                let max = content_w.saturating_sub(viewport_w);
                let new_x = scroll.x.saturating_add(step).min(max);
                if new_x != scroll.x {
                    scroll.x = new_x;
                    changed = true;
                }
            }
            _ => {}
        }

        if changed {
            self.scroll = scroll;
        }
        changed
    }
}

#[derive(Clone, Debug)]
struct TableBlockState {
    headers: Vec<Vec<InlineSpan>>,
    rows: Vec<Vec<Vec<InlineSpan>>>,
    col_widths: Vec<u16>,
    scroll: ScrollOffset,
}

impl TableBlockState {
    fn new(headers: Vec<Vec<InlineSpan>>, rows: Vec<Vec<Vec<InlineSpan>>>) -> Self {
        let mut col_widths = Vec::new();
        let col_count = headers
            .len()
            .max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
        col_widths.resize(col_count, 0);

        for (idx, cell) in headers.iter().enumerate() {
            col_widths[idx] = col_widths[idx].max(spans_width(cell));
        }
        for row in rows.iter() {
            for (idx, cell) in row.iter().enumerate() {
                col_widths[idx] = col_widths[idx].max(spans_width(cell));
            }
        }

        Self {
            headers,
            rows,
            col_widths,
            scroll: ScrollOffset::ZERO,
        }
    }

    fn content_size(&self) -> (u16, u16) {
        let col_total: u16 = self.col_widths.iter().map(|w| w.saturating_add(2)).sum();
        let width = col_total.saturating_add(self.col_widths.len().saturating_add(1) as u16);
        let mut height: u16 = 0;
        if width == 0 {
            return (0, 0);
        }
        height = height.saturating_add(1); // top border
        if !self.headers.is_empty() {
            height = height.saturating_add(1); // header row
            height = height.saturating_add(1); // separator
        }
        height = height.saturating_add(self.rows.len().min(u16::MAX as usize) as u16);
        height = height.saturating_add(1); // bottom border
        (width, height)
    }

    fn handle_scroll(
        &mut self,
        m: MouseEvent,
        viewport_w: u16,
        viewport_h: u16,
        step: u16,
    ) -> bool {
        let (content_w, content_h) = self.content_size();
        let mut scroll = self.scroll;
        let mut changed = false;

        let kind = normalize_wheel_kind(m.kind, m.modifiers);
        match kind {
            MouseEventKind::ScrollUp => {
                let dy = step as i16;
                let new_y = scroll.y.saturating_sub(dy as u16);
                if new_y != scroll.y {
                    scroll.y = new_y;
                    changed = true;
                }
            }
            MouseEventKind::ScrollDown => {
                let max = content_h.saturating_sub(viewport_h);
                let new_y = scroll.y.saturating_add(step).min(max);
                if new_y != scroll.y {
                    scroll.y = new_y;
                    changed = true;
                }
            }
            MouseEventKind::ScrollLeft => {
                let dx = step as i16;
                let new_x = scroll.x.saturating_sub(dx as u16);
                if new_x != scroll.x {
                    scroll.x = new_x;
                    changed = true;
                }
            }
            MouseEventKind::ScrollRight => {
                let max = content_w.saturating_sub(viewport_w);
                let new_x = scroll.x.saturating_add(step).min(max);
                if new_x != scroll.x {
                    scroll.x = new_x;
                    changed = true;
                }
            }
            _ => {}
        }

        if changed {
            self.scroll = scroll;
        }
        changed
    }

    fn link_at(&self, col: u16, row: u16) -> Option<String> {
        let (_, height) = self.content_size();
        if row >= height {
            return None;
        }
        let line = self.scroll.y.saturating_add(row);
        let spans = table_line_raw_spans(self, line);
        let col = self.scroll.x.saturating_add(col);
        link_at_in_spans(&spans, col)
    }
}

fn normalize_wheel_kind(kind: MouseEventKind, modifiers: KeyModifiers) -> MouseEventKind {
    if modifiers.contains(KeyModifiers::SHIFT) {
        match kind {
            MouseEventKind::ScrollUp => MouseEventKind::ScrollLeft,
            MouseEventKind::ScrollDown => MouseEventKind::ScrollRight,
            _ => kind,
        }
    } else {
        kind
    }
}

#[derive(Clone, Copy, Debug)]
struct EmbeddedScrollView {
    show_v: bool,
    show_h: bool,
    viewport_w: u16,
    viewport_h: u16,
}

impl EmbeddedScrollView {
    const THICKNESS: u16 = 1;

    fn solve_auto(content: (u16, u16), outer: (u16, u16)) -> Self {
        let (content_w, content_h) = content;
        let (outer_w, outer_h) = outer;

        let mut show_v = false;
        let mut show_h = false;

        // Two-pass solve: scrollbar visibility affects viewport size, which can affect the other
        // scrollbar's visibility (e.g. vbar steals width, causing hbar).
        for _ in 0..2 {
            let viewport_w = outer_w.saturating_sub(if show_v { Self::THICKNESS } else { 0 });
            let viewport_h = outer_h.saturating_sub(if show_h { Self::THICKNESS } else { 0 });

            let can_show_v = outer_w > Self::THICKNESS && viewport_h > 0;
            let can_show_h = outer_h > Self::THICKNESS && viewport_w > 0;

            let new_show_v = can_show_v
                && should_show_scrollbar(ScrollbarVisibility::Auto, content_h, viewport_h);
            let new_show_h = can_show_h
                && should_show_scrollbar(ScrollbarVisibility::Auto, content_w, viewport_w);

            if new_show_v == show_v && new_show_h == show_h {
                break;
            }
            show_v = new_show_v;
            show_h = new_show_h;
        }

        let viewport_w = outer_w.saturating_sub(if show_v { Self::THICKNESS } else { 0 });
        let viewport_h = outer_h.saturating_sub(if show_h { Self::THICKNESS } else { 0 });

        Self {
            show_v,
            show_h,
            viewport_w,
            viewport_h,
        }
    }
}

#[derive(Clone, Debug)]
struct MarkdownStyles {
    base: Style,
    heading: [Style; 6],
    bold: Style,
    italic: Style,
    strike: Style,
    blockquote: Style,
    list_bullet: Style,
    code_inline: Style,
    code_block: Style,
    table_border: Style,
    table_border_glyphs: TableBorderGlyphs,
    table_header: Style,
    table_cell: Style,
    link: Style,
    marker: Style,
}

impl MarkdownStyles {
    fn resolve(theme: &Theme, shared: &MarkdownShared) -> Self {
        let base_fallback = theme.window_bg.patch(theme.widget.normal);
        let mut base = theme.named_style("markdown-base").unwrap_or(base_fallback);
        if let Some(fg) = shared.fg_override.get() {
            base = base.fg(fg);
        }
        if let Some(bg) = shared.bg_override.get() {
            base = base.bg(bg);
        }

        let heading_default = |lvl: u8| {
            let mut style = Style::default().add_modifier(Modifier::BOLD);
            if lvl <= 2 {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            style
        };

        let mut heading = [base; 6];
        for (idx, slot) in heading.iter_mut().enumerate() {
            let key = format!("markdown-heading-{}", idx + 1);
            let fallback = heading_default((idx + 1) as u8);
            *slot = base.patch(theme.named_style(&key).unwrap_or(fallback));
        }

        Self {
            base,
            heading,
            bold: theme
                .named_style("markdown-bold")
                .unwrap_or(Style::default().add_modifier(Modifier::BOLD)),
            italic: theme
                .named_style("markdown-italic")
                .unwrap_or(Style::default().add_modifier(Modifier::ITALIC)),
            strike: theme
                .named_style("markdown-strikethrough")
                .unwrap_or(Style::default().add_modifier(Modifier::CROSSED_OUT)),
            blockquote: base.patch(
                theme
                    .named_style("markdown-blockquote")
                    .unwrap_or(theme.widget.dim),
            ),
            list_bullet: base.patch(
                theme
                    .named_style("markdown-list-bullet")
                    .unwrap_or(theme.widget.accent),
            ),
            code_inline: base.patch(
                theme
                    .named_style("markdown-code-inline")
                    .unwrap_or(theme.widget.accent),
            ),
            code_block: base.patch(theme.named_style("markdown-code-block").unwrap_or(base)),
            table_border: theme
                .named_style("markdown-table-border")
                .unwrap_or(theme.widget.dim),
            table_border_glyphs: TableBorderGlyphs::from_theme(theme),
            table_header: base.patch(
                theme
                    .named_style("markdown-table-header")
                    .unwrap_or(theme.widget.accent.add_modifier(Modifier::BOLD)),
            ),
            table_cell: base.patch(theme.named_style("markdown-table-cell").unwrap_or(base)),
            link: base.patch(
                theme
                    .named_style("markdown-link")
                    .unwrap_or(theme.widget.accent.add_modifier(Modifier::UNDERLINED)),
            ),
            marker: base.patch(
                theme
                    .named_style("markdown-mark")
                    .unwrap_or(theme.widget.dim),
            ),
        }
    }
}

#[derive(Clone, Debug)]
struct TableBorderGlyphs {
    top_left: String,
    top_right: String,
    bottom_left: String,
    bottom_right: String,
    horizontal: String,
    vertical: String,
    top_join: String,
    bottom_join: String,
    left_join: String,
    right_join: String,
    center_join: String,
}

impl TableBorderGlyphs {
    fn from_theme(theme: &Theme) -> Self {
        let horizontal = theme.glyph("h-border").unwrap_or("─").to_string();
        let vertical = theme.glyph("v-border").unwrap_or("│").to_string();
        let top_left = theme.glyph("top-left-corner").unwrap_or("┌").to_string();
        let top_right = theme.glyph("top-right-corner").unwrap_or("┐").to_string();
        let bottom_left = theme.glyph("bottom-left-corner").unwrap_or("└").to_string();
        let bottom_right = theme
            .glyph("bottom-right-corner")
            .unwrap_or("┘")
            .to_string();

        let is_double = horizontal == "═"
            || vertical == "║"
            || top_left == "╔"
            || top_right == "╗"
            || bottom_left == "╚"
            || bottom_right == "╝";
        let is_ascii = horizontal == "-" || vertical == "|" || top_left == "+";

        let (top_join, bottom_join, left_join, right_join, center_join) = if is_double {
            ("╦", "╩", "╠", "╣", "╬")
        } else if is_ascii {
            ("+", "+", "+", "+", "+")
        } else {
            ("┬", "┴", "├", "┤", "┼")
        };

        Self {
            top_left,
            top_right,
            bottom_left,
            bottom_right,
            horizontal,
            vertical,
            top_join: top_join.to_string(),
            bottom_join: bottom_join.to_string(),
            left_join: left_join.to_string(),
            right_join: right_join.to_string(),
            center_join: center_join.to_string(),
        }
    }
}

fn parse_markdown(input: &str, show_markers: bool) -> Vec<MdBlock> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let parser = Parser::new_ext(input, options);
    let mut state = ParserState::new(show_markers);

    for event in parser {
        state.handle_event(event);
    }

    state.finish()
}

struct ParserState {
    show_markers: bool,
    stack: Vec<Container>,
    inline_style: InlineStyleState,
    current_block: Option<CurrentBlock>,
    code_block: Option<CodeBlockBuffer>,
    next_code_id: usize,
    next_table_id: usize,
}

impl ParserState {
    fn new(show_markers: bool) -> Self {
        Self {
            show_markers,
            stack: vec![Container::Root(Vec::new())],
            inline_style: InlineStyleState::default(),
            current_block: None,
            code_block: None,
            next_code_id: 0,
            next_table_id: 0,
        }
    }

    fn finish(mut self) -> Vec<MdBlock> {
        while self.stack.len() > 1 {
            let child = self.stack.pop().unwrap();
            self.push_container(child);
        }

        match self.stack.pop().unwrap() {
            Container::Root(blocks) => blocks,
            _ => Vec::new(),
        }
    }

    fn handle_event(&mut self, event: MdEvent) {
        match event {
            MdEvent::Start(tag) => self.handle_start(tag),
            MdEvent::End(tag) => self.handle_end(tag),
            MdEvent::Text(text) => self.push_text(text.as_ref()),
            MdEvent::Code(text) => self.push_code(text.as_ref()),
            MdEvent::SoftBreak => self.push_text(" "),
            MdEvent::HardBreak => self.push_text("\n"),
            MdEvent::Rule => {
                let spans = vec![InlineSpan::marker("---")];
                self.push_block(MdBlock::Paragraph(spans));
            }
            _ => {}
        }
    }

    fn handle_start(&mut self, tag: Tag) {
        if self.code_block.is_some() {
            return;
        }

        match tag {
            Tag::Paragraph => self.start_block(CurrentBlockKind::Paragraph),
            Tag::Heading { level, .. } => {
                let level = heading_level_to_u8(level);
                self.start_block(CurrentBlockKind::Heading(level));
                if self.show_markers {
                    let marker = "#".repeat(level as usize) + " ";
                    self.push_span(InlineSpan::marker(&marker));
                }
            }
            Tag::BlockQuote(_) => self.stack.push(Container::BlockQuote(Vec::new())),
            Tag::List(start) => self.stack.push(Container::List(ListState::new(start))),
            Tag::Item => self
                .stack
                .push(Container::ListItem(ListItem { blocks: Vec::new() })),
            Tag::CodeBlock(kind) => {
                let info = match kind {
                    CodeBlockKind::Fenced(info) => Some(info.into_string()),
                    CodeBlockKind::Indented => None,
                };
                self.code_block = Some(CodeBlockBuffer {
                    info,
                    text: String::new(),
                });
            }
            Tag::Table(_) => self.stack.push(Container::Table(TableState::new())),
            Tag::TableHead => {
                if let Some(table) = self.table_state_mut() {
                    table.in_head = true;
                }
            }
            Tag::TableRow => {
                if let Some(table) = self.table_state_mut() {
                    table.current_row = Vec::new();
                }
            }
            Tag::TableCell => self.start_block(CurrentBlockKind::TableCell),
            Tag::Emphasis => {
                if self.show_markers {
                    self.push_span(InlineSpan::marker("*"));
                }
                self.inline_style.italic += 1;
            }
            Tag::Strong => {
                if self.show_markers {
                    self.push_span(InlineSpan::marker("**"));
                }
                self.inline_style.bold += 1;
            }
            Tag::Strikethrough => {
                if self.show_markers {
                    self.push_span(InlineSpan::marker("~~"));
                }
                self.inline_style.strike += 1;
            }
            Tag::Link { dest_url, .. } => {
                let url = dest_url.into_string();
                if self.show_markers {
                    self.push_span(InlineSpan::marker("["));
                }
                self.inline_style.link_stack.push(url);
            }
            _ => {}
        }
    }

    fn handle_end(&mut self, tag: TagEnd) {
        if self.code_block.is_some() {
            if matches!(tag, TagEnd::CodeBlock)
                && let Some(code) = self.code_block.take()
            {
                let id = self.next_code_id;
                self.next_code_id += 1;
                self.push_block(MdBlock::CodeBlock {
                    id,
                    info: code.info,
                    text: code.text,
                });
            }
            return;
        }

        match tag {
            TagEnd::Paragraph => self.finish_block(),
            TagEnd::Heading(_) => self.finish_block(),
            TagEnd::BlockQuote => {
                if let Some(container) = self.stack.pop() {
                    self.push_container(container);
                }
            }
            TagEnd::List(_) => {
                if let Some(container) = self.stack.pop() {
                    self.push_container(container);
                }
            }
            TagEnd::Item => {
                if let Some(Container::ListItem(item)) = self.stack.pop() {
                    if let Some(Container::List(list)) = self.stack.last_mut() {
                        list.items.push(item);
                    } else {
                        self.push_block(MdBlock::List {
                            ordered: false,
                            start: 1,
                            items: vec![item],
                        });
                    }
                }
            }
            TagEnd::Table => {
                if let Some(container) = self.stack.pop() {
                    self.push_container(container);
                }
            }
            TagEnd::TableHead => {
                if let Some(table) = self.table_state_mut() {
                    table.in_head = false;
                }
            }
            TagEnd::TableRow => {
                if let Some(table) = self.table_state_mut() {
                    let row = std::mem::take(&mut table.current_row);
                    if table.in_head && table.headers.is_empty() {
                        table.headers = row;
                    } else {
                        table.rows.push(row);
                    }
                }
            }
            TagEnd::TableCell => self.finish_block(),
            TagEnd::Emphasis => {
                self.inline_style.italic = self.inline_style.italic.saturating_sub(1);
                if self.show_markers {
                    self.push_span(InlineSpan::marker("*"));
                }
            }
            TagEnd::Strong => {
                self.inline_style.bold = self.inline_style.bold.saturating_sub(1);
                if self.show_markers {
                    self.push_span(InlineSpan::marker("**"));
                }
            }
            TagEnd::Strikethrough => {
                self.inline_style.strike = self.inline_style.strike.saturating_sub(1);
                if self.show_markers {
                    self.push_span(InlineSpan::marker("~~"));
                }
            }
            TagEnd::Link => {
                let url = self.inline_style.link_stack.pop();
                if let Some(url) = url
                    && self.show_markers
                {
                    self.push_span(InlineSpan::marker("]("));
                    self.push_span(InlineSpan::text(
                        &url,
                        InlineStyle::default(),
                        Some(url.clone()),
                    ));
                    self.push_span(InlineSpan::marker(")"));
                }
            }
            _ => {}
        }
    }

    fn push_text(&mut self, text: &str) {
        if let Some(code) = &mut self.code_block {
            code.text.push_str(text);
            return;
        }
        if let Some(span) = self.text_span(text) {
            self.push_span(span);
        }
    }

    fn push_code(&mut self, text: &str) {
        if self.code_block.is_some() {
            self.push_text(text);
            return;
        }
        let style = InlineStyle {
            code: true,
            ..Default::default()
        };
        if self.show_markers {
            self.push_span(InlineSpan::marker("`"));
        }
        self.push_span(InlineSpan::text(text, style, self.inline_style.link()));
        if self.show_markers {
            self.push_span(InlineSpan::marker("`"));
        }
    }

    fn text_span(&self, text: &str) -> Option<InlineSpan> {
        if text.is_empty() {
            return None;
        }
        Some(InlineSpan::text(
            text,
            self.inline_style.current(),
            self.inline_style.link(),
        ))
    }

    fn push_span(&mut self, span: InlineSpan) {
        if let Some(block) = &mut self.current_block {
            block.spans.push(span);
        }
    }

    fn start_block(&mut self, block: CurrentBlockKind) {
        self.current_block = Some(CurrentBlock {
            kind: block,
            spans: Vec::new(),
        });
    }

    fn finish_block(&mut self) {
        let Some(block) = self.current_block.take() else {
            return;
        };
        let spans = block.spans;
        match block.kind {
            CurrentBlockKind::Paragraph => self.push_block(MdBlock::Paragraph(spans)),
            CurrentBlockKind::Heading(level) => self.push_block(MdBlock::Heading { level, spans }),
            CurrentBlockKind::TableCell => {
                if let Some(table) = self.table_state_mut() {
                    table.current_row.push(spans);
                }
            }
        }
    }

    fn push_container(&mut self, container: Container) {
        match container {
            Container::BlockQuote(blocks) => self.push_block(MdBlock::BlockQuote(blocks)),
            Container::List(list) => self.push_block(MdBlock::List {
                ordered: list.ordered,
                start: list.start,
                items: list.items,
            }),
            Container::Table(table) => {
                let id = self.next_table_id;
                self.next_table_id += 1;
                self.push_block(MdBlock::Table {
                    id,
                    headers: table.headers,
                    rows: table.rows,
                });
            }
            Container::Root(blocks) => {
                for block in blocks {
                    self.push_block(block);
                }
            }
            Container::ListItem(item) => self.push_block(MdBlock::List {
                ordered: false,
                start: 1,
                items: vec![item],
            }),
        }
    }

    fn push_block(&mut self, block: MdBlock) {
        if let Some(Container::ListItem(item)) = self.stack.last_mut() {
            item.blocks.push(block);
            return;
        }
        if let Some(Container::BlockQuote(blocks)) = self.stack.last_mut() {
            blocks.push(block);
            return;
        }
        if let Some(Container::Root(blocks)) = self.stack.last_mut() {
            blocks.push(block);
            return;
        }

        if let Some(Container::List(list)) = self.stack.last_mut() {
            list.items.push(ListItem {
                blocks: vec![block],
            });
            return;
        }

        self.stack.push(Container::Root(vec![block]));
    }

    fn table_state_mut(&mut self) -> Option<&mut TableState> {
        match self.stack.last_mut() {
            Some(Container::Table(table)) => Some(table),
            _ => None,
        }
    }
}

#[derive(Default)]
struct InlineStyleState {
    bold: u8,
    italic: u8,
    strike: u8,
    link_stack: Vec<String>,
}

impl InlineStyleState {
    fn current(&self) -> InlineStyle {
        InlineStyle {
            bold: self.bold > 0,
            italic: self.italic > 0,
            strike: self.strike > 0,
            code: false,
        }
    }

    fn link(&self) -> Option<String> {
        self.link_stack.last().cloned()
    }
}

struct CurrentBlock {
    kind: CurrentBlockKind,
    spans: Vec<InlineSpan>,
}

#[derive(Clone, Copy)]
enum CurrentBlockKind {
    Paragraph,
    Heading(u8),
    TableCell,
}

enum Container {
    Root(Vec<MdBlock>),
    BlockQuote(Vec<MdBlock>),
    List(ListState),
    ListItem(ListItem),
    Table(TableState),
}

struct ListState {
    ordered: bool,
    start: u64,
    items: Vec<ListItem>,
}

impl ListState {
    fn new(start: Option<u64>) -> Self {
        let ordered = start.is_some();
        let start = start.unwrap_or(1);
        Self {
            ordered,
            start,
            items: Vec::new(),
        }
    }
}

struct TableState {
    headers: Vec<Vec<InlineSpan>>,
    rows: Vec<Vec<Vec<InlineSpan>>>,
    current_row: Vec<Vec<InlineSpan>>,
    in_head: bool,
}

impl TableState {
    fn new() -> Self {
        Self {
            headers: Vec::new(),
            rows: Vec::new(),
            current_row: Vec::new(),
            in_head: false,
        }
    }
}

struct CodeBlockBuffer {
    info: Option<String>,
    text: String,
}

fn build_block_states(blocks: &[MdBlock]) -> (Vec<CodeBlockState>, Vec<TableBlockState>) {
    let mut codes = Vec::new();
    let mut tables = Vec::new();
    collect_block_states(blocks, &mut codes, &mut tables);
    (codes, tables)
}

fn collect_block_states(
    blocks: &[MdBlock],
    codes: &mut Vec<CodeBlockState>,
    tables: &mut Vec<TableBlockState>,
) {
    for block in blocks {
        match block {
            MdBlock::CodeBlock { text, .. } => {
                codes.push(CodeBlockState::new(text));
            }
            MdBlock::Table { headers, rows, .. } => {
                tables.push(TableBlockState::new(headers.clone(), rows.clone()));
            }
            MdBlock::BlockQuote(inner) => collect_block_states(inner, codes, tables),
            MdBlock::List { items, .. } => {
                for item in items {
                    collect_block_states(&item.blocks, codes, tables);
                }
            }
            _ => {}
        }
    }
}

#[derive(Clone, Debug)]
struct RenderContext {
    layers: Vec<PrefixLayer>,
}

impl RenderContext {
    fn root() -> Self {
        Self { layers: Vec::new() }
    }

    fn push_quote(&mut self) {
        self.layers.push(PrefixLayer::Quote);
    }

    fn pop_quote(&mut self) {
        if matches!(self.layers.last(), Some(PrefixLayer::Quote)) {
            self.layers.pop();
        }
    }

    fn push_list_item(&mut self, bullet: String) {
        let depth = self
            .layers
            .iter()
            .filter(|layer| matches!(layer, PrefixLayer::List { .. }))
            .count()
            .saturating_add(1);
        let indent = " ".repeat(LIST_INDENT_SPACES.saturating_mul(depth.saturating_sub(1)));
        self.layers.push(PrefixLayer::List {
            indent,
            bullet,
            used: false,
        });
    }

    fn pop_list_item(&mut self) {
        if matches!(self.layers.last(), Some(PrefixLayer::List { .. })) {
            self.layers.pop();
        }
    }

    fn mark_list_prefix_used(&mut self) {
        if let Some(layer) = self
            .layers
            .iter_mut()
            .rev()
            .find(|layer| matches!(layer, PrefixLayer::List { .. }))
            && let PrefixLayer::List { used, .. } = layer
        {
            *used = true;
        }
    }

    fn is_blockquote(&self) -> bool {
        self.layers
            .iter()
            .any(|layer| matches!(layer, PrefixLayer::Quote))
    }
}

#[derive(Clone, Debug)]
enum PrefixLayer {
    Quote,
    List {
        indent: String,
        bullet: String,
        used: bool,
    },
}

fn build_layout(
    blocks: &[MdBlock],
    wrap_width: u16,
    max_code_height: u16,
    max_table_height: u16,
    show_markers: bool,
    code_blocks: &[CodeBlockState],
    tables: &[TableBlockState],
) -> Layout {
    let mut builder = LayoutBuilder::new(wrap_width);
    let mut ctx = RenderContext::root();
    render_block_list(
        blocks,
        wrap_width,
        max_code_height,
        max_table_height,
        show_markers,
        code_blocks,
        tables,
        &mut ctx,
        &mut builder,
    );
    builder.finish()
}

struct LayoutBuilder {
    wrap_width: u16,
    blocks: Vec<LayoutBlock>,
    link_hits: Vec<LinkHit>,
    cursor_y: u16,
}

impl LayoutBuilder {
    fn new(wrap_width: u16) -> Self {
        Self {
            wrap_width,
            blocks: Vec::new(),
            link_hits: Vec::new(),
            cursor_y: 0,
        }
    }

    fn finish(self) -> Layout {
        Layout {
            wrap_width: self.wrap_width,
            blocks: self.blocks,
            total_height: self.cursor_y,
            link_hits: self.link_hits,
        }
    }

    fn add_spacing(&mut self, lines: u16) {
        if lines == 0 {
            return;
        }
        self.cursor_y = self.cursor_y.saturating_add(lines);
    }

    fn push_text_block(&mut self, lines: Vec<LineLayout>, style: TextBlockStyle) {
        if lines.is_empty() {
            return;
        }
        let y = self.cursor_y;
        let height = lines.len().min(u16::MAX as usize) as u16;
        let block = LayoutBlock {
            y,
            height,
            kind: LayoutBlockKind::Text {
                lines: lines.clone(),
                style,
            },
        };
        for (idx, line) in lines.iter().enumerate() {
            let row = y.saturating_add(idx as u16);
            let mut col: u16 = 0;
            for span in &line.spans {
                let w = text_width(&span.text);
                if let Some(url) = &span.link
                    && w > 0
                {
                    self.link_hits.push(LinkHit {
                        row,
                        start: col,
                        end: col.saturating_add(w),
                        url: url.clone(),
                    });
                }
                col = col.saturating_add(w);
            }
        }
        self.cursor_y = self.cursor_y.saturating_add(height);
        self.blocks.push(block);
    }

    fn push_code_block(
        &mut self,
        index: usize,
        prefix: PrefixSpec,
        in_blockquote: bool,
        height: u16,
    ) {
        let y = self.cursor_y;
        let block = LayoutBlock {
            y,
            height: height.max(1),
            kind: LayoutBlockKind::Code {
                index,
                prefix,
                in_blockquote,
            },
        };
        self.cursor_y = self.cursor_y.saturating_add(block.height);
        self.blocks.push(block);
    }

    fn push_table_block(
        &mut self,
        index: usize,
        prefix: PrefixSpec,
        in_blockquote: bool,
        height: u16,
    ) {
        let y = self.cursor_y;
        let block = LayoutBlock {
            y,
            height: height.max(1),
            kind: LayoutBlockKind::Table {
                index,
                prefix,
                in_blockquote,
            },
        };
        self.cursor_y = self.cursor_y.saturating_add(block.height);
        self.blocks.push(block);
    }
}

#[allow(clippy::too_many_arguments)]
fn render_block_list(
    blocks: &[MdBlock],
    wrap_width: u16,
    max_code_height: u16,
    max_table_height: u16,
    show_markers: bool,
    code_blocks: &[CodeBlockState],
    tables: &[TableBlockState],
    ctx: &mut RenderContext,
    builder: &mut LayoutBuilder,
) {
    let mut first = true;
    for block in blocks {
        if !first {
            builder.add_spacing(1);
        }
        first = false;
        render_block(
            block,
            wrap_width,
            max_code_height,
            max_table_height,
            show_markers,
            code_blocks,
            tables,
            ctx,
            builder,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_block(
    block: &MdBlock,
    wrap_width: u16,
    max_code_height: u16,
    max_table_height: u16,
    show_markers: bool,
    code_blocks: &[CodeBlockState],
    tables: &[TableBlockState],
    ctx: &mut RenderContext,
    builder: &mut LayoutBuilder,
) {
    match block {
        MdBlock::Paragraph(spans) => {
            let prefix = build_prefix(ctx);
            let lines = wrap_with_prefix(spans, wrap_width, &prefix);
            let style = TextBlockStyle {
                kind: TextKind::Paragraph,
                in_blockquote: ctx.is_blockquote(),
            };
            builder.push_text_block(lines, style);
            ctx.mark_list_prefix_used();
        }
        MdBlock::Heading { level, spans } => {
            let prefix = build_prefix(ctx);
            let lines = wrap_with_prefix(spans, wrap_width, &prefix);
            let style = TextBlockStyle {
                kind: TextKind::Heading(*level),
                in_blockquote: ctx.is_blockquote(),
            };
            builder.push_text_block(lines, style);
            ctx.mark_list_prefix_used();
        }
        MdBlock::CodeBlock { id, info, .. } => {
            if show_markers {
                let prefix = build_prefix(ctx);
                let fence = if let Some(info) = info.as_ref() {
                    InlineSpan::marker(&format!("```{}", info))
                } else {
                    InlineSpan::marker("```")
                };
                let lines = wrap_with_prefix(&[fence], wrap_width, &prefix);
                let style = TextBlockStyle {
                    kind: TextKind::Paragraph,
                    in_blockquote: ctx.is_blockquote(),
                };
                builder.push_text_block(lines, style);
                ctx.mark_list_prefix_used();
            }

            let prefix = build_prefix(ctx);
            let height = code_blocks
                .get(*id)
                .map(|code| code.content_size().1)
                .unwrap_or(1)
                .min(max_code_height)
                .max(1);
            builder.push_code_block(*id, prefix.clone(), ctx.is_blockquote(), height);
            ctx.mark_list_prefix_used();

            if show_markers {
                let prefix = build_prefix(ctx);
                let fence = InlineSpan::marker("```");
                let lines = wrap_with_prefix(&[fence], wrap_width, &prefix);
                let style = TextBlockStyle {
                    kind: TextKind::Paragraph,
                    in_blockquote: ctx.is_blockquote(),
                };
                builder.push_text_block(lines, style);
                ctx.mark_list_prefix_used();
            }
        }
        MdBlock::Table { id, .. } => {
            let prefix = build_prefix(ctx);
            let height = tables
                .get(*id)
                .map(|table| table.content_size().1)
                .unwrap_or(1)
                .min(max_table_height)
                .max(1);
            builder.push_table_block(*id, prefix, ctx.is_blockquote(), height);
            ctx.mark_list_prefix_used();
        }
        MdBlock::BlockQuote(inner) => {
            ctx.push_quote();
            render_block_list(
                inner,
                wrap_width,
                max_code_height,
                max_table_height,
                show_markers,
                code_blocks,
                tables,
                ctx,
                builder,
            );
            ctx.pop_quote();
        }
        MdBlock::List {
            ordered,
            start,
            items,
        } => {
            let mut idx = *start;
            for (item_idx, item) in items.iter().enumerate() {
                if item_idx > 0 {
                    builder.add_spacing(0);
                }
                let bullet = if *ordered {
                    format!("{}.", idx)
                } else {
                    "-".to_string()
                };
                idx = idx.saturating_add(1);
                ctx.push_list_item(bullet);
                render_block_list(
                    &item.blocks,
                    wrap_width,
                    max_code_height,
                    max_table_height,
                    show_markers,
                    code_blocks,
                    tables,
                    ctx,
                    builder,
                );
                ctx.pop_list_item();
            }
        }
    }
}

fn build_prefix(ctx: &RenderContext) -> PrefixSpec {
    let mut first = Vec::new();
    let mut rest = Vec::new();

    for layer in ctx.layers.iter() {
        match layer {
            PrefixLayer::Quote => {
                first.push(InlineSpan::text("> ", InlineStyle::default(), None));
                rest.push(InlineSpan::text("> ", InlineStyle::default(), None));
            }
            PrefixLayer::List {
                indent,
                bullet,
                used,
            } => {
                if !indent.is_empty() {
                    first.push(InlineSpan::text(indent, InlineStyle::default(), None));
                    rest.push(InlineSpan::text(indent, InlineStyle::default(), None));
                }
                let bullet_width = text_width(bullet);
                if !*used {
                    first.push(InlineSpan::bullet(bullet));
                    first.push(InlineSpan::text(" ", InlineStyle::default(), None));
                } else {
                    let pad = " ".repeat((bullet_width + 1) as usize);
                    first.push(InlineSpan::text(&pad, InlineStyle::default(), None));
                }
                let pad = " ".repeat((bullet_width + 1) as usize);
                rest.push(InlineSpan::text(&pad, InlineStyle::default(), None));
            }
        }
    }

    let first_width = spans_width(&first);
    let rest_width = spans_width(&rest);
    PrefixSpec {
        first,
        rest,
        first_width,
        rest_width,
    }
}

fn wrap_with_prefix(spans: &[InlineSpan], width: u16, prefix: &PrefixSpec) -> Vec<LineLayout> {
    let mut tokens = Vec::new();
    for span in spans {
        tokens.extend(tokenize_span(span));
    }

    wrap_tokens(tokens, width, prefix)
}

fn wrap_tokens(tokens: Vec<Token>, width: u16, prefix: &PrefixSpec) -> Vec<LineLayout> {
    let mut lines = Vec::new();
    let mut current = LineLayout {
        spans: prefix.first.clone(),
        width: prefix.first_width,
    };
    let mut current_prefix_width = prefix.first_width;

    for token in tokens {
        match token.kind {
            TokenKind::Newline => {
                lines.push(current);
                current = LineLayout {
                    spans: prefix.rest.clone(),
                    width: prefix.rest_width,
                };
                current_prefix_width = prefix.rest_width;
            }
            TokenKind::Space => {
                if current.width == current_prefix_width {
                    continue;
                }
                let token_width = text_width(&token.text);
                if current.width.saturating_add(token_width) > width {
                    lines.push(current);
                    current = LineLayout {
                        spans: prefix.rest.clone(),
                        width: prefix.rest_width,
                    };
                    current_prefix_width = prefix.rest_width;
                    continue;
                }
                current.width = current.width.saturating_add(token_width);
                current.spans.push(token.span);
            }
            TokenKind::Text => {
                let span_template = token.span;
                let mut remaining_text = token.text;
                loop {
                    let available = width.saturating_sub(current.width).max(1);
                    let token_width = text_width(&remaining_text);
                    if token_width <= available {
                        current.width = current.width.saturating_add(token_width);
                        let mut span = span_template.clone();
                        span.text = remaining_text;
                        current.spans.push(span);
                        break;
                    }
                    let (head, head_width, tail) = split_text_at_width(&remaining_text, available);
                    if head_width > 0 {
                        current.width = current.width.saturating_add(head_width);
                        let mut span = span_template.clone();
                        span.text = head;
                        current.spans.push(span);
                    }
                    lines.push(current);
                    current = LineLayout {
                        spans: prefix.rest.clone(),
                        width: prefix.rest_width,
                    };
                    current_prefix_width = prefix.rest_width;
                    if tail.is_empty() {
                        break;
                    }
                    remaining_text = tail;
                }
            }
        }
    }

    if !current.spans.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(LineLayout {
            spans: prefix.first.clone(),
            width: prefix.first_width,
        });
    }

    lines
}

#[derive(Clone)]
struct Token {
    text: String,
    span: InlineSpan,
    kind: TokenKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Text,
    Space,
    Newline,
}

fn tokenize_span(span: &InlineSpan) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut buffer = String::new();
    let mut kind = None;

    for ch in span.text.chars() {
        let next_kind = match ch {
            '\n' => TokenKind::Newline,
            '\t' => TokenKind::Space,
            c if c.is_whitespace() => TokenKind::Space,
            _ => TokenKind::Text,
        };

        if next_kind == TokenKind::Newline {
            if !buffer.is_empty() {
                tokens.push(Token {
                    text: buffer.clone(),
                    span: InlineSpan {
                        text: buffer.clone(),
                        ..span.clone()
                    },
                    kind: kind.unwrap_or(TokenKind::Text),
                });
                buffer.clear();
                kind = None;
            }
            tokens.push(Token {
                text: "\n".to_string(),
                span: InlineSpan {
                    text: "\n".to_string(),
                    ..span.clone()
                },
                kind: TokenKind::Newline,
            });
            continue;
        }

        let effective_char = if ch == '\t' { ' ' } else { ch };

        if kind.is_none() || kind == Some(next_kind) {
            buffer.push(effective_char);
            kind = Some(next_kind);
            continue;
        }

        if !buffer.is_empty() {
            tokens.push(Token {
                text: buffer.clone(),
                span: InlineSpan {
                    text: buffer.clone(),
                    ..span.clone()
                },
                kind: kind.unwrap_or(TokenKind::Text),
            });
            buffer.clear();
        }
        buffer.push(effective_char);
        kind = Some(next_kind);
    }

    if !buffer.is_empty() {
        tokens.push(Token {
            text: buffer.clone(),
            span: InlineSpan {
                text: buffer,
                ..span.clone()
            },
            kind: kind.unwrap_or(TokenKind::Text),
        });
    }

    tokens
}

fn draw_line(
    frame: &mut Frame<'_>,
    x: u16,
    y: u16,
    width: u16,
    line: &LineLayout,
    block_style: &TextBlockStyle,
    styles: &MarkdownStyles,
) {
    if width == 0 {
        return;
    }
    let base = base_style_for_block(block_style, styles);
    let spans = styled_spans(&line.spans, base, styles);
    draw_spans_with_scroll(frame, x, y, width, &spans, 0);
}

#[allow(clippy::too_many_arguments)]
fn draw_code_block(
    frame: &mut Frame<'_>,
    area: Rect,
    block: &LayoutBlock,
    code: &mut CodeBlockState,
    prefix: &PrefixSpec,
    scroll: ScrollOffset,
    wrap_width: u16,
    styles: &MarkdownStyles,
    theme: &Theme,
    in_blockquote: bool,
) {
    let prefix_width = prefix.first_width.max(prefix.rest_width);
    let total_width = wrap_width.min(area.width);
    let content_x = area.x.saturating_add(prefix_width);
    let content_width = total_width.saturating_sub(prefix_width);
    if content_width == 0 {
        return;
    }

    let block_start = block.y;
    let block_end = block.y.saturating_add(block.height);
    let visible_start = block_start.max(scroll.y);
    let visible_end = block_end.min(scroll.y.saturating_add(area.height));
    if visible_start >= visible_end {
        return;
    }

    let prefix_style = if in_blockquote {
        styles.blockquote
    } else {
        styles.base
    };
    let code_style = styles.code_block;

    let (content_w, content_h) = code.content_size();
    let embedded =
        EmbeddedScrollView::solve_auto((content_w, content_h), (content_width, block.height));
    let viewport_w = embedded.viewport_w;
    let viewport_h = embedded.viewport_h;

    let max_x = content_w.saturating_sub(viewport_w);
    let max_y = content_h.saturating_sub(viewport_h);
    let content_scroll = ScrollOffset {
        x: code.scroll.x.min(max_x),
        y: code.scroll.y.min(max_y),
    };
    if content_scroll != code.scroll {
        code.scroll = content_scroll;
    }

    let arrows = true;
    let v_layout = embedded.show_v.then_some(scrollbar_layout_1d(
        viewport_h,
        viewport_h,
        content_h,
        content_scroll.y,
        arrows,
    ));
    let h_layout = embedded.show_h.then_some(scrollbar_layout_1d(
        viewport_w,
        viewport_w,
        content_w,
        content_scroll.x,
        arrows,
    ));

    let track_style = theme.scrollbar_track;
    let thumb_style = theme.scrollbar_thumb;
    let arrow_style = theme.scrollbar_arrow;
    let track = theme.glyph("scrollbar-track").unwrap_or("░");
    let thumb = theme.glyph("scrollbar-thumb").unwrap_or("█");
    let arrow_up = theme.glyph("scrollbar-up-arrow").unwrap_or("▲");
    let arrow_down = theme.glyph("scrollbar-down-arrow").unwrap_or("▼");
    let arrow_left = theme.glyph("scrollbar-left-arrow").unwrap_or("◄");
    let arrow_right = theme.glyph("scrollbar-right-arrow").unwrap_or("►");

    for line_offset in visible_start..visible_end {
        let local_line = line_offset.saturating_sub(block_start);
        let screen_y = area.y.saturating_add(line_offset.saturating_sub(scroll.y));
        let prefix_spans = if local_line == 0 {
            &prefix.first
        } else {
            &prefix.rest
        };
        let styled_prefix = styled_prefix_spans(prefix_spans, prefix_style, styles);
        draw_spans_with_scroll(frame, area.x, screen_y, prefix_width, &styled_prefix, 0);

        if embedded.show_h && local_line >= viewport_h {
            let Some(layout) = h_layout else {
                continue;
            };

            let buf = frame.buffer_mut();
            for dx in 0..viewport_w {
                let (symbol, bar_style) = if layout.has_arrows && dx == 0 {
                    (arrow_left, arrow_style)
                } else if layout.has_arrows && dx == layout.bar_len.saturating_sub(1) {
                    (arrow_right, arrow_style)
                } else if dx >= layout.thumb_start
                    && dx < layout.thumb_start.saturating_add(layout.thumb_len)
                {
                    (thumb, thumb_style)
                } else {
                    (track, track_style)
                };
                if let Some(cell) = buf.cell_mut((content_x.saturating_add(dx), screen_y)) {
                    cell.set_symbol(symbol);
                    cell.set_style(code_style.patch(bar_style));
                }
            }

            if embedded.show_v {
                if let Some(cell) = buf.cell_mut((content_x.saturating_add(viewport_w), screen_y)) {
                    cell.set_symbol(track);
                    cell.set_style(code_style.patch(track_style));
                }
            }
            continue;
        }

        let code_line_idx = content_scroll.y.saturating_add(local_line);
        let line = code
            .lines
            .get(code_line_idx as usize)
            .cloned()
            .unwrap_or_default();
        fill_line(frame, content_x, screen_y, viewport_w, code_style);
        let (segment, _) = slice_by_width(&line, content_scroll.x, viewport_w);
        let styled = vec![StyledSpan {
            text: segment,
            style: code_style,
        }];
        draw_spans_with_scroll(frame, content_x, screen_y, viewport_w, &styled, 0);

        if embedded.show_v {
            let Some(layout) = v_layout else {
                continue;
            };

            let dy = local_line.min(layout.bar_len.saturating_sub(1));
            let (symbol, bar_style) = if layout.has_arrows && dy == 0 {
                (arrow_up, arrow_style)
            } else if layout.has_arrows && dy == layout.bar_len.saturating_sub(1) {
                (arrow_down, arrow_style)
            } else if dy >= layout.thumb_start
                && dy < layout.thumb_start.saturating_add(layout.thumb_len)
            {
                (thumb, thumb_style)
            } else {
                (track, track_style)
            };

            let buf = frame.buffer_mut();
            if let Some(cell) = buf.cell_mut((content_x.saturating_add(viewport_w), screen_y)) {
                cell.set_symbol(symbol);
                cell.set_style(code_style.patch(bar_style));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_table_block(
    frame: &mut Frame<'_>,
    area: Rect,
    block: &LayoutBlock,
    table: &mut TableBlockState,
    prefix: &PrefixSpec,
    scroll: ScrollOffset,
    wrap_width: u16,
    styles: &MarkdownStyles,
    theme: &Theme,
    in_blockquote: bool,
) {
    let prefix_width = prefix.first_width.max(prefix.rest_width);
    let total_width = wrap_width.min(area.width);
    let content_x = area.x.saturating_add(prefix_width);
    let content_width = total_width.saturating_sub(prefix_width);
    if content_width == 0 {
        return;
    }

    let (table_width, table_height) = table.content_size();
    if table_width == 0 || table_height == 0 {
        return;
    }

    let block_start = block.y;
    let block_end = block.y.saturating_add(block.height);
    let visible_start = block_start.max(scroll.y);
    let visible_end = block_end.min(scroll.y.saturating_add(area.height));
    if visible_start >= visible_end {
        return;
    }

    let prefix_style = if in_blockquote {
        styles.blockquote
    } else {
        styles.base
    };

    let (content_w, content_h) = table.content_size();
    let embedded =
        EmbeddedScrollView::solve_auto((content_w, content_h), (content_width, block.height));
    let viewport_w = embedded.viewport_w;
    let viewport_h = embedded.viewport_h;

    let max_x = content_w.saturating_sub(viewport_w);
    let max_y = content_h.saturating_sub(viewport_h);
    let content_scroll = ScrollOffset {
        x: table.scroll.x.min(max_x),
        y: table.scroll.y.min(max_y),
    };
    if content_scroll != table.scroll {
        table.scroll = content_scroll;
    }

    let arrows = true;
    let v_layout = embedded.show_v.then_some(scrollbar_layout_1d(
        viewport_h,
        viewport_h,
        content_h,
        content_scroll.y,
        arrows,
    ));
    let h_layout = embedded.show_h.then_some(scrollbar_layout_1d(
        viewport_w,
        viewport_w,
        content_w,
        content_scroll.x,
        arrows,
    ));

    let track_style = theme.scrollbar_track;
    let thumb_style = theme.scrollbar_thumb;
    let arrow_style = theme.scrollbar_arrow;
    let track = theme.glyph("scrollbar-track").unwrap_or("░");
    let thumb = theme.glyph("scrollbar-thumb").unwrap_or("█");
    let arrow_up = theme.glyph("scrollbar-up-arrow").unwrap_or("▲");
    let arrow_down = theme.glyph("scrollbar-down-arrow").unwrap_or("▼");
    let arrow_left = theme.glyph("scrollbar-left-arrow").unwrap_or("◄");
    let arrow_right = theme.glyph("scrollbar-right-arrow").unwrap_or("►");

    for line_offset in visible_start..visible_end {
        let local_line = line_offset.saturating_sub(block_start);
        let screen_y = area.y.saturating_add(line_offset.saturating_sub(scroll.y));

        let prefix_spans = if local_line == 0 {
            &prefix.first
        } else {
            &prefix.rest
        };
        let styled_prefix = styled_prefix_spans(prefix_spans, prefix_style, styles);
        draw_spans_with_scroll(frame, area.x, screen_y, prefix_width, &styled_prefix, 0);

        if embedded.show_h && local_line >= viewport_h {
            let Some(layout) = h_layout else {
                continue;
            };

            let buf = frame.buffer_mut();
            for dx in 0..viewport_w {
                let (symbol, bar_style) = if layout.has_arrows && dx == 0 {
                    (arrow_left, arrow_style)
                } else if layout.has_arrows && dx == layout.bar_len.saturating_sub(1) {
                    (arrow_right, arrow_style)
                } else if dx >= layout.thumb_start
                    && dx < layout.thumb_start.saturating_add(layout.thumb_len)
                {
                    (thumb, thumb_style)
                } else {
                    (track, track_style)
                };
                if let Some(cell) = buf.cell_mut((content_x.saturating_add(dx), screen_y)) {
                    cell.set_symbol(symbol);
                    cell.set_style(styles.table_cell.patch(bar_style));
                }
            }

            if embedded.show_v {
                if let Some(cell) = buf.cell_mut((content_x.saturating_add(viewport_w), screen_y)) {
                    cell.set_symbol(track);
                    cell.set_style(styles.table_cell.patch(track_style));
                }
            }
            continue;
        }

        let table_line = content_scroll.y.saturating_add(local_line);
        let spans = table_line_spans(table, table_line, styles);
        draw_spans_with_scroll(
            frame,
            content_x,
            screen_y,
            viewport_w,
            &spans,
            content_scroll.x,
        );

        if embedded.show_v {
            let Some(layout) = v_layout else {
                continue;
            };

            let dy = local_line.min(layout.bar_len.saturating_sub(1));
            let (symbol, bar_style) = if layout.has_arrows && dy == 0 {
                (arrow_up, arrow_style)
            } else if layout.has_arrows && dy == layout.bar_len.saturating_sub(1) {
                (arrow_down, arrow_style)
            } else if dy >= layout.thumb_start
                && dy < layout.thumb_start.saturating_add(layout.thumb_len)
            {
                (thumb, thumb_style)
            } else {
                (track, track_style)
            };

            let buf = frame.buffer_mut();
            if let Some(cell) = buf.cell_mut((content_x.saturating_add(viewport_w), screen_y)) {
                cell.set_symbol(symbol);
                cell.set_style(styles.table_cell.patch(bar_style));
            }
        }
    }
}

fn table_line_spans(
    table: &TableBlockState,
    line: u16,
    styles: &MarkdownStyles,
) -> Vec<StyledSpan> {
    let (width, height) = table.content_size();
    if line >= height || width == 0 {
        return Vec::new();
    }

    let mut line_idx = 0u16;

    if line == line_idx {
        return border_line_spans(table, TableBorderLineKind::Top, styles);
    }
    line_idx = line_idx.saturating_add(1);

    if !table.headers.is_empty() {
        if line == line_idx {
            return row_line_spans(table, &table.headers, true, styles);
        }
        line_idx = line_idx.saturating_add(1);
        if line == line_idx {
            return border_line_spans(table, TableBorderLineKind::Middle, styles);
        }
        line_idx = line_idx.saturating_add(1);
    }

    let body_index = line.saturating_sub(line_idx);
    if body_index < table.rows.len() as u16 {
        return row_line_spans(table, &table.rows[body_index as usize], false, styles);
    }

    border_line_spans(table, TableBorderLineKind::Bottom, styles)
}

fn table_line_raw_spans(table: &TableBlockState, line: u16) -> Vec<InlineSpan> {
    let (width, height) = table.content_size();
    if line >= height || width == 0 {
        return Vec::new();
    }

    let mut line_idx = 0u16;

    if line == line_idx {
        return border_line_raw_spans(table);
    }
    line_idx = line_idx.saturating_add(1);

    if !table.headers.is_empty() {
        if line == line_idx {
            return row_line_raw_spans(table, &table.headers);
        }
        line_idx = line_idx.saturating_add(1);
        if line == line_idx {
            return border_line_raw_spans(table);
        }
        line_idx = line_idx.saturating_add(1);
    }

    let body_index = line.saturating_sub(line_idx);
    if body_index < table.rows.len() as u16 {
        return row_line_raw_spans(table, &table.rows[body_index as usize]);
    }

    border_line_raw_spans(table)
}

fn border_line_raw_spans(table: &TableBlockState) -> Vec<InlineSpan> {
    if table.col_widths.is_empty() {
        return Vec::new();
    }
    let mut text = String::new();
    text.push('+');
    for width in &table.col_widths {
        let cell_w = width.saturating_add(2);
        text.push_str(&"-".repeat(cell_w as usize));
        text.push('+');
    }
    vec![InlineSpan::marker(&text)]
}

fn row_line_raw_spans(table: &TableBlockState, row: &[Vec<InlineSpan>]) -> Vec<InlineSpan> {
    let mut spans = Vec::new();
    spans.push(InlineSpan::marker("|"));
    for (col_idx, width) in table.col_widths.iter().enumerate() {
        spans.push(InlineSpan::text(" ", InlineStyle::default(), None));
        let cell = row.get(col_idx).cloned().unwrap_or_default();
        let cell_width = spans_width(&cell);
        spans.extend(cell);
        let pad = width.saturating_sub(cell_width);
        if pad > 0 {
            spans.push(InlineSpan::text(
                &" ".repeat(pad as usize),
                InlineStyle::default(),
                None,
            ));
        }
        spans.push(InlineSpan::text(" ", InlineStyle::default(), None));
        spans.push(InlineSpan::marker("|"));
    }
    spans
}

#[derive(Clone, Copy, Debug)]
enum TableBorderLineKind {
    Top,
    Middle,
    Bottom,
}

fn border_line_spans(
    table: &TableBlockState,
    kind: TableBorderLineKind,
    styles: &MarkdownStyles,
) -> Vec<StyledSpan> {
    if table.col_widths.is_empty() {
        return Vec::new();
    }
    let glyphs = &styles.table_border_glyphs;
    let (left, join, right) = match kind {
        TableBorderLineKind::Top => (&glyphs.top_left, &glyphs.top_join, &glyphs.top_right),
        TableBorderLineKind::Middle => (&glyphs.left_join, &glyphs.center_join, &glyphs.right_join),
        TableBorderLineKind::Bottom => (
            &glyphs.bottom_left,
            &glyphs.bottom_join,
            &glyphs.bottom_right,
        ),
    };
    let mut text = String::new();
    text.push_str(left);
    for (idx, width) in table.col_widths.iter().enumerate() {
        let cell_w = width.saturating_add(2);
        text.push_str(&glyphs.horizontal.repeat(cell_w as usize));
        if idx + 1 < table.col_widths.len() {
            text.push_str(join);
        } else {
            text.push_str(right);
        }
    }
    vec![StyledSpan {
        text,
        style: styles.table_border,
    }]
}

fn row_line_spans(
    table: &TableBlockState,
    row: &[Vec<InlineSpan>],
    is_header: bool,
    styles: &MarkdownStyles,
) -> Vec<StyledSpan> {
    let mut spans = Vec::new();
    let border_style = styles.table_border;
    let vbar = styles.table_border_glyphs.vertical.clone();
    let cell_style = if is_header {
        styles.table_header
    } else {
        styles.table_cell
    };

    spans.push(StyledSpan {
        text: vbar.clone(),
        style: border_style,
    });

    for (col_idx, width) in table.col_widths.iter().enumerate() {
        spans.push(StyledSpan {
            text: " ".to_string(),
            style: cell_style,
        });
        let cell = row.get(col_idx).cloned().unwrap_or_default();
        let base_spans = styled_spans(&cell, cell_style, styles);
        spans.extend(base_spans);
        let cell_width = spans_width(&cell);
        let pad = width.saturating_sub(cell_width);
        if pad > 0 {
            spans.push(StyledSpan {
                text: " ".repeat(pad as usize),
                style: cell_style,
            });
        }
        spans.push(StyledSpan {
            text: " ".to_string(),
            style: cell_style,
        });
        spans.push(StyledSpan {
            text: vbar.clone(),
            style: border_style,
        });
    }

    spans
}

fn styled_prefix_spans(
    spans: &[InlineSpan],
    base: Style,
    styles: &MarkdownStyles,
) -> Vec<StyledSpan> {
    spans
        .iter()
        .map(|span| {
            let style = match span.kind {
                SpanKind::Bullet => base.patch(styles.list_bullet),
                SpanKind::Marker => base.patch(styles.marker),
                SpanKind::Text => base,
            };
            StyledSpan {
                text: span.text.clone(),
                style,
            }
        })
        .collect()
}

fn styled_spans(spans: &[InlineSpan], base: Style, styles: &MarkdownStyles) -> Vec<StyledSpan> {
    let mut out = Vec::new();
    for span in spans {
        let mut style = base;
        match span.kind {
            SpanKind::Marker => {
                style = style.patch(styles.marker);
            }
            SpanKind::Bullet => {
                style = style.patch(styles.list_bullet);
            }
            SpanKind::Text => {
                if span.inline.code {
                    style = style.patch(styles.code_inline);
                }
                if span.inline.bold {
                    style = style.patch(styles.bold);
                }
                if span.inline.italic {
                    style = style.patch(styles.italic);
                }
                if span.inline.strike {
                    style = style.patch(styles.strike);
                }
                if span.link.is_some() {
                    style = style.patch(styles.link);
                }
            }
        }
        out.push(StyledSpan {
            text: span.text.clone(),
            style,
        });
    }
    out
}

fn base_style_for_block(style: &TextBlockStyle, styles: &MarkdownStyles) -> Style {
    let base = if style.in_blockquote {
        styles.blockquote
    } else {
        styles.base
    };

    match style.kind {
        TextKind::Paragraph => base,
        TextKind::Heading(level) => {
            let idx = cmp::min((level.saturating_sub(1)) as usize, 5);
            base.patch(styles.heading[idx])
        }
    }
}

#[derive(Clone)]
struct StyledSpan {
    text: String,
    style: Style,
}

fn draw_spans_with_scroll(
    frame: &mut Frame<'_>,
    x: u16,
    y: u16,
    width: u16,
    spans: &[StyledSpan],
    scroll_x: u16,
) {
    if width == 0 {
        return;
    }
    let mut drawn: u16 = 0;
    let mut offset: u16 = 0;
    for span in spans {
        if drawn >= width {
            break;
        }
        let span_width = text_width(&span.text);
        if offset.saturating_add(span_width) <= scroll_x {
            offset = offset.saturating_add(span_width);
            continue;
        }
        let start = scroll_x.saturating_sub(offset);
        let available = width.saturating_sub(drawn);
        let (segment, segment_width) = slice_by_width(&span.text, start, available);
        if segment_width == 0 {
            offset = offset.saturating_add(span_width);
            continue;
        }
        frame.buffer_mut().set_stringn(
            x.saturating_add(drawn),
            y,
            segment,
            segment_width as usize,
            span.style,
        );
        drawn = drawn.saturating_add(segment_width);
        offset = offset.saturating_add(span_width);
    }
}

fn fill_line(frame: &mut Frame<'_>, x: u16, y: u16, width: u16, style: Style) {
    let buf = frame.buffer_mut();
    for dx in 0..width {
        if let Some(cell) = buf.cell_mut((x.saturating_add(dx), y)) {
            cell.set_symbol(" ");
            cell.set_style(style);
        }
    }
}

fn slice_by_width(s: &str, start: u16, max_width: u16) -> (String, u16) {
    if s.is_empty() || max_width == 0 {
        return (String::new(), 0);
    }
    let mut result = String::new();
    let mut width: u16 = 0;
    let mut col: u16 = 0;

    for g in UnicodeSegmentation::graphemes(s, true) {
        let w = UnicodeWidthStr::width(g).max(1) as u16;
        let next = col.saturating_add(w);
        if next <= start {
            col = next;
            continue;
        }
        if width.saturating_add(w) > max_width {
            break;
        }
        result.push_str(g);
        width = width.saturating_add(w);
        col = next;
    }

    (result, width)
}

fn split_text_at_width(s: &str, max_width: u16) -> (String, u16, String) {
    if s.is_empty() || max_width == 0 {
        return (String::new(), 0, s.to_string());
    }
    let mut head = String::new();
    let mut tail = String::new();
    let mut width: u16 = 0;
    let mut in_tail = false;

    for g in UnicodeSegmentation::graphemes(s, true) {
        let w = UnicodeWidthStr::width(g).max(1) as u16;
        if !in_tail && width.saturating_add(w) <= max_width {
            head.push_str(g);
            width = width.saturating_add(w);
        } else {
            in_tail = true;
            tail.push_str(g);
        }
    }

    (head, width, tail)
}

fn spans_width(spans: &[InlineSpan]) -> u16 {
    spans.iter().map(|span| text_width(&span.text)).sum()
}

fn link_at_in_spans(spans: &[InlineSpan], col: u16) -> Option<String> {
    let mut offset: u16 = 0;
    for span in spans {
        let width = text_width(&span.text);
        if let Some(url) = &span.link
            && col >= offset
            && col < offset.saturating_add(width)
        {
            return Some(url.clone());
        }
        offset = offset.saturating_add(width);
    }
    None
}

fn text_width(s: &str) -> u16 {
    UnicodeWidthStr::width(s).min(u16::MAX as usize) as u16
}

fn normalize_tabs(s: &str) -> String {
    s.replace('\t', "    ")
}

fn heading_level_to_u8(level: pulldown_cmark::HeadingLevel) -> u8 {
    use pulldown_cmark::HeadingLevel;
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

impl InlineSpan {
    fn text(text: &str, inline: InlineStyle, link: Option<String>) -> Self {
        Self {
            text: text.to_string(),
            inline,
            link,
            kind: SpanKind::Text,
        }
    }

    fn marker(text: &str) -> Self {
        Self {
            text: text.to_string(),
            inline: InlineStyle::default(),
            link: None,
            kind: SpanKind::Marker,
        }
    }

    fn bullet(text: &str) -> Self {
        Self {
            text: text.to_string(),
            inline: InlineStyle::default(),
            link: None,
            kind: SpanKind::Bullet,
        }
    }
}

impl Default for InlineSpan {
    fn default() -> Self {
        Self {
            text: String::new(),
            inline: InlineStyle::default(),
            link: None,
            kind: SpanKind::Text,
        }
    }
}
