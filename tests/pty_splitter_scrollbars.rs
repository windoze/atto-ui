use std::time::Duration;

use atto_ui_test_host::PtyTestHost;

#[test]
fn pty_splitter_child_scrollbars_mount_on_split_borders() -> anyhow::Result<()> {
    let bin = env!("CARGO_BIN_EXE_snapshot_splitter_scrollbars_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24)?;

    host.wait_for_text("L000:", Duration::from_secs(2))?;
    host.wait_for_text("R000:", Duration::from_secs(2))?;

    // For an 80x24 PTY:
    // - work area starts at (0,1) due to the menubar, so the window is at (2,3).
    // - window size is 50x14, so its inner rect is (3,4) with size 48x12.
    // - splitter is vertical with divider thickness 1:
    //   available = 48 - 1 = 47, first_len = 47/2 = 23, divider column = inner.x + 23 = 26.
    let inner_x = 3u16;
    let inner_y = 4u16;
    let inner_w = 48u16;
    let inner_h = 12u16;

    let divider_x = inner_x + 23;
    let right_border_x = inner_x + inner_w - 1;
    let bottom_y = inner_y + inner_h - 1;
    let v_arrow_down_y = inner_y + inner_h - 2;

    // Left pane vertical scrollbar is hosted on the split divider column.
    assert_eq!(host.cell_contents(divider_x, inner_y)?, "▲");
    assert_eq!(host.cell_contents(divider_x, v_arrow_down_y)?, "▼");

    // Bottom horizontal scrollbars are split by the divider: left half ends before divider,
    // right half starts after divider.
    assert_eq!(host.cell_contents(inner_x, bottom_y)?, "◄");
    assert_eq!(host.cell_contents(divider_x, bottom_y)?, "░");
    assert_eq!(host.cell_contents(divider_x + 1, bottom_y)?, "◄");

    // Right pane vertical scrollbar is hosted on the pane's right edge (inside the window).
    assert_eq!(host.cell_contents(right_border_x, inner_y)?, "▲");
    assert_eq!(host.cell_contents(right_border_x, v_arrow_down_y)?, "▼");

    // When both scrollbars are visible, the bottom-right corner is reserved for the corner cell.
    assert_eq!(host.cell_contents(right_border_x, bottom_y)?, "░");

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}
