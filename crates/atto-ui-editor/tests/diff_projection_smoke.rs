use editor_core_diff::LineDiffConfig;
use editor_core_diff_view::{DiffMode, DiffModel, RowSlot};

const BEFORE: &str = "ctx top one\nREMOVED_LINE gone\nctx shared two\n";
const AFTER: &str = "ctx top one\nctx shared two\nADDED_LINE fresh\n";

#[test]
fn side_by_side_before_column_has_cells() {
    let model = DiffModel::from_before_after(BEFORE, AFTER, LineDiffConfig::default());
    let projection =
        editor_core_diff_view::DiffProjection::build(&model, DiffMode::SideBySide, &[40, 40]);

    let mut col0_text = String::new();
    let mut col1_text = String::new();
    for row in projection.rows() {
        if let Some(RowSlot::Line { cells, .. }) = row.slots().first() {
            for c in cells {
                col0_text.push(c.ch);
            }
            col0_text.push('\n');
        }
        if let Some(RowSlot::Line { cells, .. }) = row.slots().get(1) {
            for c in cells {
                col1_text.push(c.ch);
            }
            col1_text.push('\n');
        }
    }

    eprintln!("--- col0 (before) ---\n{col0_text}");
    eprintln!("--- col1 (after) ---\n{col1_text}");
    assert!(
        col0_text.contains("REMOVED_LINE"),
        "before column missing removed line"
    );
    assert!(
        col1_text.contains("ADDED_LINE"),
        "after column missing added line"
    );
}
