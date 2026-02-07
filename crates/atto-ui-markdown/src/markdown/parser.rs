use pulldown_cmark::{CodeBlockKind, Event as MdEvent, Options, Parser, Tag, TagEnd};

use super::embedded_scrollbar::{CodeBlockState, TableBlockState};
use super::text::text_width;

#[derive(Clone, Debug)]
pub(super) enum MdBlock {
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
pub(super) struct ListItem {
    pub(super) blocks: Vec<MdBlock>,
}

#[derive(Clone, Debug)]
pub(super) struct InlineSpan {
    pub(super) text: String,
    pub(super) inline: InlineStyle,
    pub(super) link: Option<String>,
    pub(super) kind: SpanKind,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct InlineStyle {
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) strike: bool,
    pub(super) code: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SpanKind {
    Text,
    Marker,
    Bullet,
}

pub(super) fn parse_markdown(input: &str, show_markers: bool) -> Vec<MdBlock> {
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

    fn start_block(&mut self, kind: CurrentBlockKind) {
        self.current_block = Some(CurrentBlock {
            kind,
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

pub(super) fn build_block_states(
    blocks: &[MdBlock],
) -> (Vec<CodeBlockState>, Vec<TableBlockState>) {
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

pub(super) fn spans_width(spans: &[InlineSpan]) -> u16 {
    spans.iter().map(|span| text_width(&span.text)).sum()
}

pub(super) fn link_at_in_spans(spans: &[InlineSpan], col: u16) -> Option<String> {
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
    pub(super) fn text(text: &str, inline: InlineStyle, link: Option<String>) -> Self {
        Self {
            text: text.to_string(),
            inline,
            link,
            kind: SpanKind::Text,
        }
    }

    pub(super) fn marker(text: &str) -> Self {
        Self {
            text: text.to_string(),
            inline: InlineStyle::default(),
            link: None,
            kind: SpanKind::Marker,
        }
    }

    pub(super) fn bullet(text: &str) -> Self {
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
