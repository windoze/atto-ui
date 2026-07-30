//! Menu-bar construction, status-segment building, and the chat-window rect helper.

use crate::*;

pub(crate) fn agent_menu(quit_events: EventQueue<()>) -> MenuBar {
    // Keep the initial app shell minimal while still offering a discoverable quit action.
    MenuBar::new(vec![MenuSpec::new(
        "File",
        vec![MenuItem::action("Quit", move || quit_events.push(())).shortcut("q")],
    )])
}

pub(crate) fn status_segments(bindings: StatusSegmentBindings) -> Vec<StatusSegment> {
    vec![
        StatusSegment::new("app", APP_TITLE)
            .priority(40)
            .min_width(10),
        StatusSegment::new("provider", bindings.provider)
            .priority(86)
            .min_width(18),
        StatusSegment::new("model", bindings.model)
            .priority(95)
            .min_width(18),
        StatusSegment::new("plan", bindings.plan_mode)
            .priority(94)
            .min_width(9),
        StatusSegment::new("tools", bindings.tools)
            .priority(93)
            .min_width(8),
        StatusSegment::new("skills", bindings.skills)
            .priority(92)
            .min_width(9),
        StatusSegment::new("tokens", bindings.tokens)
            .priority(91)
            .min_width(8),
        StatusSegment::new("error", bindings.error)
            .align(StatusSegmentAlign::Right)
            .priority(89)
            .min_width(6),
        StatusSegment::new("streaming", bindings.state)
            .align(StatusSegmentAlign::Right)
            .priority(90)
            .min_width(9),
        StatusSegment::new("keys", "Esc cancel | Ctrl+Q quit | /help")
            .align(StatusSegmentAlign::Right)
            .priority(30)
            .min_width(28),
    ]
}

pub(crate) fn chat_window_rect(screen: Rect) -> Rect {
    // Fill the desktop work area with a small margin on normal terminal sizes.
    let work = Desktop::layout(screen).work_area;
    let margin_x = u16::from(work.width > 48);
    let margin_y = u16::from(work.height > 16);
    Rect {
        x: work.x.saturating_add(margin_x),
        y: work.y.saturating_add(margin_y),
        width: work.width.saturating_sub(margin_x.saturating_mul(2)),
        height: work.height.saturating_sub(margin_y.saturating_mul(2)),
    }
}
