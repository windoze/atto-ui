use super::layout::LayoutBlockKind;
use super::parser::{MdBlock, SpanKind};

#[test]
fn parser_heading_markers_respect_show_markers_flag() {
    let md = "# Hello";

    let blocks = super::parser::parse_markdown(md, false);
    assert_eq!(blocks.len(), 1);
    let MdBlock::Heading { level, spans } = &blocks[0] else {
        panic!("expected heading block");
    };
    assert_eq!(*level, 1);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].text, "Hello");
    assert_eq!(spans[0].kind, SpanKind::Text);

    let blocks = super::parser::parse_markdown(md, true);
    assert_eq!(blocks.len(), 1);
    let MdBlock::Heading { level, spans } = &blocks[0] else {
        panic!("expected heading block");
    };
    assert_eq!(*level, 1);
    assert!(spans.len() >= 2);
    assert_eq!(spans[0].text, "# ");
    assert_eq!(spans[0].kind, SpanKind::Marker);
}

#[test]
fn layout_clamps_code_block_height_and_optionally_renders_fences() {
    let md = "```txt\nline1\nline2\nline3\n```\n";

    // Without markers, we expect only a single "code" layout block.
    let blocks = super::parser::parse_markdown(md, false);
    let (codes, tables) = super::parser::build_block_states(&blocks);
    let layout = super::layout::build_layout(&blocks, 80, 2, 8, false, &codes, &tables);
    assert_eq!(layout.blocks.len(), 1);
    match &layout.blocks[0].kind {
        LayoutBlockKind::Code { .. } => {}
        _ => panic!("expected code layout block"),
    }
    assert_eq!(layout.blocks[0].height, 2);
    assert_eq!(layout.total_height, 2);

    // With markers, we expect the opening fence, the code block, and the closing fence.
    let blocks = super::parser::parse_markdown(md, true);
    let (codes, tables) = super::parser::build_block_states(&blocks);
    let layout = super::layout::build_layout(&blocks, 80, 2, 8, true, &codes, &tables);
    assert_eq!(layout.blocks.len(), 3);
    assert_eq!(layout.total_height, 4);
    assert!(matches!(
        layout.blocks[1].kind,
        LayoutBlockKind::Code { .. }
    ));
}

#[test]
fn tolerant_parser_renders_unclosed_fence_as_code_block() {
    let blocks = super::parser::parse_markdown_tolerant("```rust\nfn main() {", false);
    assert_eq!(blocks.len(), 1);
    let MdBlock::CodeBlock { info, text, .. } = &blocks[0] else {
        panic!("expected tolerant code block");
    };
    assert_eq!(info.as_deref(), Some("rust"));
    assert_eq!(text, "fn main() {");
}

#[test]
fn tolerant_parser_downgrades_trailing_incomplete_table_to_text() {
    let blocks =
        super::parser::parse_markdown_tolerant("| Name | Value |\n| --- | --- |\n| half |", false);
    assert_eq!(blocks.len(), 1);
    let MdBlock::Paragraph(spans) = &blocks[0] else {
        panic!("expected trailing incomplete table to stay plain text");
    };
    let rendered = spans
        .iter()
        .map(|span| span.text.as_str())
        .collect::<String>();
    assert!(rendered.contains("| Name | Value |"));
    assert!(rendered.contains("| half |"));
}

#[test]
fn tolerant_parser_keeps_table_like_text_inside_unclosed_fence() {
    let blocks = super::parser::parse_markdown_tolerant(
        "```text\n\n| Name | Value |\n| --- | --- |\n| half |",
        false,
    );

    assert_eq!(blocks.len(), 1);
    let MdBlock::CodeBlock { info, text, .. } = &blocks[0] else {
        panic!("expected unclosed fence to remain a code block");
    };
    assert_eq!(info.as_deref(), Some("text"));
    assert!(text.contains("| Name | Value |"));
    assert!(text.contains("| half |"));
    assert!(!text.contains("\\|"));
}

#[test]
fn tolerant_parser_keeps_completed_table_as_table() {
    let blocks = super::parser::parse_markdown_tolerant(
        "| Name | Value |\n| --- | --- |\n| half | stable |\n",
        false,
    );
    assert_eq!(blocks.len(), 1);
    let MdBlock::Table { headers, rows, .. } = &blocks[0] else {
        panic!("expected completed table block");
    };
    assert_eq!(headers.len(), 2);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].len(), 2);
}

#[test]
fn unclosed_code_block_text_can_be_replaced_incrementally() {
    let mut blocks = super::parser::parse_markdown_tolerant("```text\none", false);
    let next = super::parser::unclosed_fenced_code_block("```text\none\ntwo")
        .expect("unclosed fence should be detected");

    assert!(super::parser::replace_last_code_block_text(
        &mut blocks,
        next.text
    ));

    let MdBlock::CodeBlock { text, .. } = &blocks[0] else {
        panic!("expected code block");
    };
    assert_eq!(text, "one\ntwo");
}
