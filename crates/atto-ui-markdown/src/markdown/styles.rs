use ratatui::style::{Color, Modifier, Style};

use crate::syntax::SyntaxClass;
use atto_ui::theme::Theme;

use super::MarkdownShared;

#[derive(Clone, Debug)]
pub(super) struct MarkdownStyles {
    pub(super) base: Style,
    pub(super) heading: [Style; 6],
    pub(super) bold: Style,
    pub(super) italic: Style,
    pub(super) strike: Style,
    pub(super) blockquote: Style,
    pub(super) list_bullet: Style,
    pub(super) code_inline: Style,
    pub(super) code_block: Style,
    pub(super) syntax: SyntaxStyles,
    pub(super) table_border: Style,
    pub(super) table_border_glyphs: TableBorderGlyphs,
    pub(super) table_header: Style,
    pub(super) table_cell: Style,
    pub(super) link: Style,
    pub(super) marker: Style,
}

impl MarkdownStyles {
    pub(super) fn resolve(theme: &Theme, shared: &MarkdownShared) -> Self {
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
            syntax: SyntaxStyles::from_theme(theme),
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
pub(super) struct SyntaxStyles {
    text: Style,
    comment: Style,
    string: Style,
    keyword: Style,
    function: Style,
    ty: Style,
    number: Style,
    constant: Style,
    variable: Style,
    operator: Style,
    punctuation: Style,
}

impl SyntaxStyles {
    fn from_theme(theme: &Theme) -> Self {
        Self {
            text: theme
                .named_style("markdown-syntax-text")
                .unwrap_or_default(),
            comment: theme
                .named_style("markdown-syntax-comment")
                .unwrap_or(Style::default().fg(Color::DarkGray)),
            string: theme
                .named_style("markdown-syntax-string")
                .unwrap_or(Style::default().fg(Color::LightGreen)),
            keyword: theme
                .named_style("markdown-syntax-keyword")
                .unwrap_or(Style::default().fg(Color::LightMagenta)),
            function: theme
                .named_style("markdown-syntax-function")
                .unwrap_or(Style::default().fg(Color::LightCyan)),
            ty: theme
                .named_style("markdown-syntax-type")
                .unwrap_or(Style::default().fg(Color::Yellow)),
            number: theme
                .named_style("markdown-syntax-number")
                .unwrap_or(Style::default().fg(Color::LightYellow)),
            constant: theme
                .named_style("markdown-syntax-constant")
                .unwrap_or(Style::default().fg(Color::LightYellow)),
            variable: theme
                .named_style("markdown-syntax-variable")
                .unwrap_or_default(),
            operator: theme
                .named_style("markdown-syntax-operator")
                .unwrap_or(Style::default().fg(Color::LightBlue)),
            punctuation: theme
                .named_style("markdown-syntax-punctuation")
                .unwrap_or(Style::default().fg(Color::Gray)),
        }
    }

    pub(super) fn style_for(&self, class: SyntaxClass) -> Style {
        match class {
            SyntaxClass::Text => self.text,
            SyntaxClass::Comment => self.comment,
            SyntaxClass::String => self.string,
            SyntaxClass::Keyword => self.keyword,
            SyntaxClass::Function => self.function,
            SyntaxClass::Type => self.ty,
            SyntaxClass::Number => self.number,
            SyntaxClass::Constant => self.constant,
            SyntaxClass::Variable => self.variable,
            SyntaxClass::Operator => self.operator,
            SyntaxClass::Punctuation => self.punctuation,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct TableBorderGlyphs {
    pub(super) top_left: String,
    pub(super) top_right: String,
    pub(super) bottom_left: String,
    pub(super) bottom_right: String,
    pub(super) horizontal: String,
    pub(super) vertical: String,
    pub(super) top_join: String,
    pub(super) bottom_join: String,
    pub(super) left_join: String,
    pub(super) right_join: String,
    pub(super) center_join: String,
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
