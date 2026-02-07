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
