use super::embedded_scrollbar::{CodeBlockState, TableBlockState};
use super::parser::{InlineSpan, InlineStyle, MdBlock};
use super::text::{split_text_at_width, text_width};

#[derive(Clone, Debug)]
pub(super) struct LineLayout {
    pub(super) spans: Vec<InlineSpan>,
    pub(super) width: u16,
}

#[derive(Clone, Debug)]
pub(super) struct Layout {
    pub(super) wrap_width: u16,
    pub(super) blocks: Vec<LayoutBlock>,
    pub(super) total_height: u16,
    pub(super) link_hits: Vec<LinkHit>,
}

impl Layout {
    pub(super) fn block_at_row(&self, row: u16) -> Option<usize> {
        self.blocks
            .iter()
            .position(|block| row >= block.y && row < block.y.saturating_add(block.height))
    }

    pub(super) fn link_at(&self, col: u16, row: u16) -> Option<&LinkHit> {
        self.link_hits
            .iter()
            .find(|hit| hit.row == row && col >= hit.start && col < hit.end)
    }
}

#[derive(Clone, Debug)]
pub(super) struct LayoutBlock {
    pub(super) y: u16,
    pub(super) height: u16,
    pub(super) kind: LayoutBlockKind,
}

#[derive(Clone, Debug)]
pub(super) enum LayoutBlockKind {
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
pub(super) struct TextBlockStyle {
    pub(super) kind: TextKind,
    pub(super) in_blockquote: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum TextKind {
    Paragraph,
    Heading(u8),
}

#[derive(Clone, Debug)]
pub(super) struct PrefixSpec {
    pub(super) first: Vec<InlineSpan>,
    pub(super) rest: Vec<InlineSpan>,
    pub(super) first_width: u16,
    pub(super) rest_width: u16,
}

#[derive(Clone, Debug)]
pub(super) struct LinkHit {
    pub(super) row: u16,
    pub(super) start: u16,
    pub(super) end: u16,
    pub(super) url: String,
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
        let indent = " ".repeat(super::LIST_INDENT_SPACES.saturating_mul(depth.saturating_sub(1)));
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

pub(super) fn build_layout(
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

    let first_width = super::parser::spans_width(&first);
    let rest_width = super::parser::spans_width(&rest);
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
