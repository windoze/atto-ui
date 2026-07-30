use std::thread;
use std::time::Duration;

use ratatui::layout::Rect;

use super::*;
use crate::app::{MenuBar, MenuItem, MenuSpec};
use crate::composable::{Checkbox, ComponentTagExt, Label, TabView, TableView, VStack, Visibility};
use crate::reactive::Binding;
use crate::runtime::Rect as RuntimeRect;
use crate::theme::Theme;
use crate::wm::{Window, WindowKind};
use crate::{ComponentCommand, ComponentError, ComponentTarget, ComponentValue};

#[test]
fn inspect_tree_finds_tags() {
    let screen = Rect::new(0, 0, 80, 24);
    let menu = MenuBar::new(vec![
        MenuSpec::new(
            "File",
            vec![MenuItem::action("Open", || {}).with_tag("menu_open")],
        )
        .with_tag("menu_file"),
    ]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let view = Label::new("Hello").tag("label");
    let window = Window::new(
        WindowKind::Normal,
        "Win",
        Rect::new(2, 2, 20, 6),
        Box::new(view),
    )
    .with_tag("win1");
    desktop.add_window(window, screen);

    let mut inspector = desktop.inspect();
    let tree = inspector.tree(screen).expect("tree");
    assert!(tree.find_by_id("menu_file").is_some());
    assert!(tree.find_by_id("menu_open").is_some());
    assert!(tree.find_by_id("win1").is_some());
    assert!(tree.find_by_id("label").is_some());
}

#[test]
fn export_snapshot_contains_serializable_tree_bounds_and_text() {
    let screen = Rect::new(0, 0, 80, 24);
    let menu = MenuBar::new(vec![
        MenuSpec::new(
            "File",
            vec![MenuItem::action("Open", || {}).with_tag("menu_open")],
        )
        .with_tag("menu_file"),
    ]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let view = Label::new("Hello").tag("label");
    let window = Window::new(
        WindowKind::Normal,
        "Win",
        Rect::new(2, 2, 20, 6),
        Box::new(view),
    )
    .with_tag("win1");
    let window_id = desktop.add_window(window, screen);

    let mut inspector = desktop.inspect();
    let snapshot = inspector.export_snapshot(screen).expect("snapshot");
    serde_json::to_string(&snapshot).expect("serializable snapshot");

    assert_eq!(
        snapshot.bounds,
        RuntimeRect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        }
    );

    let menu_item = snapshot
        .tree
        .find_by_id("menu_open")
        .expect("menu item node");
    assert_eq!(menu_item.kind, NodeKind::MenuItem);
    assert_eq!(menu_item.text.as_deref(), Some("Open"));

    let win = snapshot.tree.find_by_id("win1").expect("window node");
    assert_eq!(win.kind, NodeKind::Window);
    assert_eq!(win.tag.as_deref(), Some("win1"));
    assert_eq!(win.text.as_deref(), Some("Win"));
    assert_eq!(win.state.as_deref(), Some("Normal"));
    assert_eq!(win.window_id, Some(window_id.raw()));
    assert_eq!(
        win.bounds,
        Some(RuntimeRect {
            x: 2,
            y: 2,
            width: 20,
            height: 6,
        })
    );
    assert_eq!(
        win.properties.get("focused"),
        Some(&ComponentValue::Bool(true))
    );

    let label = snapshot.tree.find_by_id("label").expect("label node");
    assert_eq!(label.kind, NodeKind::Component);
    assert_eq!(label.name, "Label");
    assert!(label.type_name.ends_with("Label"));
    assert_eq!(label.text.as_deref(), Some("Hello"));
    assert!(!label.properties.contains_key("text"));
    assert_eq!(
        label.bounds,
        Some(RuntimeRect {
            x: 3,
            y: 3,
            width: 18,
            height: 4,
        })
    );
}

#[test]
fn export_snapshot_omits_large_collection_properties() {
    let screen = Rect::new(0, 0, 80, 24);
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let rows = Binding::new(vec![vec!["a".to_string(), "b".to_string()]; 32]);
    let table = TableView::new(
        "Data",
        vec!["H1".to_string(), "H2".to_string()],
        rows,
        Binding::new(0usize),
    )
    .tag("table");
    let window = Window::new(
        WindowKind::Normal,
        "Table",
        Rect::new(1, 1, 40, 10),
        Box::new(table),
    );
    desktop.add_window(window, screen);

    let mut inspector = desktop.inspect();
    let snapshot = inspector.export_snapshot(screen).expect("snapshot");
    let table = snapshot.tree.find_by_id("table").expect("table node");

    assert_eq!(table.text.as_deref(), Some("Data"));
    assert_eq!(
        table.properties.get("enabled"),
        Some(&ComponentValue::Bool(true))
    );
    assert_eq!(
        table.properties.get("selection"),
        Some(&ComponentValue::U64(0))
    );
    assert!(!table.properties.contains_key("headers"));
    assert!(!table.properties.contains_key("rows"));
    assert!(!table.properties.contains_key("title"));
}

#[test]
fn inspect_property_names_resolves_menu_window_and_component_ids() {
    let screen = Rect::new(0, 0, 80, 24);
    let menu = MenuBar::new(vec![
        MenuSpec::new(
            "File",
            vec![MenuItem::action("Open", || {}).with_tag("menu_open")],
        )
        .with_tag("menu_file"),
    ]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let view = Label::new("Hello").tag("label");
    let window = Window::new(
        WindowKind::Normal,
        "Win",
        Rect::new(2, 2, 20, 6),
        Box::new(view),
    )
    .with_tag("win1");
    desktop.add_window(window, screen);

    let mut inspector = desktop.inspect();

    assert_eq!(
        inspector.property_names("menu_file").expect("menu spec"),
        vec!["title".to_string()]
    );
    assert_eq!(
        inspector.property_names("menu_open").expect("menu item"),
        vec![
            "label".to_string(),
            "shortcut".to_string(),
            "enabled".to_string(),
        ]
    );
    assert_eq!(
        inspector.property_names("win1").expect("window"),
        vec![
            "title".to_string(),
            "rect".to_string(),
            "state".to_string(),
            "kind".to_string(),
        ]
    );

    let component_names = inspector.property_names("label").expect("component");
    assert!(component_names.contains(&"text".to_string()));
    assert!(component_names.contains(&"enabled".to_string()));
}

#[test]
fn dispatch_precedence_prefers_window_over_component_on_tag_collision() {
    // The menu → window → component precedence now lives in `resolve_dispatch_target`. Pin it:
    // a window and one of its child components sharing a tag must resolve to the window.
    let screen = Rect::new(0, 0, 80, 24);
    let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));

    let view = Label::new("Hello").tag("shared");
    let window = Window::new(
        WindowKind::Normal,
        "Win",
        Rect::new(2, 2, 20, 6),
        Box::new(view),
    )
    .with_tag("shared");
    desktop.add_window(window, screen);

    let mut inspector = desktop.inspect();
    // Window property set (title/rect/state/kind) proves the window backend won, not the
    // component (which would expose text/enabled).
    let names = inspector.property_names("shared").expect("resolved");
    assert!(
        names.contains(&"rect".to_string()),
        "expected window backend, got {names:?}"
    );
    assert!(
        !names.contains(&"text".to_string()),
        "component must not win over window"
    );
}

#[test]
fn inspect_property_names_unknown_id_returns_not_found() {
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let mut inspector = desktop.inspect();

    assert_eq!(
        inspector.property_names("missing"),
        Err(ComponentError::NotFound("missing".to_string()))
    );
}

#[test]
fn untagged_interactive_nodes_reports_only_interactive_nodes_without_tags() {
    let screen = Rect::new(0, 0, 80, 24);
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let view = VStack::new()
        .child(Checkbox::new("Tagged", Binding::new(false)).tag("tagged_checkbox"))
        .child(Checkbox::new("Missing tag", Binding::new(false)))
        .tag("root_stack");
    let window = Window::new(
        WindowKind::Normal,
        "Checks",
        Rect::new(1, 1, 32, 8),
        Box::new(view),
    )
    .with_tag("checks_window");
    desktop.add_window(window, screen);

    let mut inspector = desktop.inspect();
    let nodes = inspector.untagged_interactive_nodes(screen);

    assert_eq!(nodes.len(), 1);
    let node = &nodes[0];
    assert_eq!(node.kind, NodeKind::Component);
    assert_eq!(node.id, None);
    assert_eq!(node.name, "Checkbox");
    assert!(node.focusable);
    assert!(node.properties.contains(&"checked".to_string()));
}

#[test]
fn desktop_change_tracker_reports_binding_changes_once() {
    let screen = Rect::new(0, 0, 80, 24);
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let text = Binding::new("Hello".to_string());
    let view = Label::new(text.clone()).tag("label");
    let window = Window::new(
        WindowKind::Normal,
        "Win",
        Rect::new(2, 2, 20, 6),
        Box::new(view),
    );
    desktop.add_window(window, screen);

    let mut tracker = desktop.inspect().change_tracker();
    assert!(!tracker.is_empty());
    assert!(!tracker.changed_since_last_poll());

    text.set("Updated".to_string());
    assert!(tracker.changed_since_last_poll());
    assert!(!tracker.changed_since_last_poll());

    text.mark_clean();
    assert!(!tracker.changed_since_last_poll());
}

#[test]
fn desktop_change_tracker_refreshes_new_binding_sources() {
    let screen = Rect::new(0, 0, 80, 24);
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let mut tracker = desktop.inspect().change_tracker();
    assert!(tracker.is_empty());

    let text = Binding::new("First".to_string());
    let view = Label::new(text.clone()).tag("label");
    let window = Window::new(
        WindowKind::Normal,
        "Win",
        Rect::new(2, 2, 20, 6),
        Box::new(view),
    );
    desktop.add_window(window, screen);

    desktop.inspect().refresh_change_tracker(&mut tracker);
    assert!(!tracker.is_empty());
    assert!(!tracker.changed_since_last_poll());

    text.set("Second".to_string());
    assert!(tracker.changed_since_last_poll());
    assert!(!tracker.changed_since_last_poll());
}

#[test]
fn invoke_checkbox_toggle_uses_semantic_dispatch_and_updates_binding() {
    let screen = Rect::new(0, 0, 80, 24);
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let checked = Binding::new(false);
    let view = Checkbox::new("Check", checked.clone()).tag("checkbox");
    let window = Window::new(
        WindowKind::Normal,
        "Checks",
        Rect::new(1, 1, 30, 6),
        Box::new(view),
    );
    desktop.add_window(window, screen);

    let mut inspector = desktop.inspect();
    let result = inspector
        .invoke(
            screen,
            ComponentTarget::Id("checkbox".to_string()),
            ComponentCommand::Toggle,
        )
        .expect("invoke");

    assert_eq!(result.dispatch, InvokeDispatch::Semantic);
    assert!(result.result.is_consumed());
    assert!(checked.get());
}

#[test]
fn query_matches_get_property_for_component_ids() {
    let screen = Rect::new(0, 0, 80, 24);
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let checked = Binding::new(true);
    let view = Checkbox::new("Check", checked).tag("checkbox");
    let window = Window::new(
        WindowKind::Normal,
        "Checks",
        Rect::new(1, 1, 30, 6),
        Box::new(view),
    );
    desktop.add_window(window, screen);

    let mut inspector = desktop.inspect();
    let queried = inspector
        .query(ComponentTarget::Id("checkbox".to_string()), "checked")
        .expect("query");
    let read = inspector
        .get_property("checkbox", "checked")
        .expect("get_property");

    assert_eq!(queried, read);
}

#[test]
fn invoke_reports_coordinate_fallback_when_no_semantic_command_exists() {
    let screen = Rect::new(0, 0, 80, 24);
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let view = Label::new("Plain").tag("label");
    let window = Window::new(
        WindowKind::Normal,
        "Labels",
        Rect::new(1, 1, 30, 6),
        Box::new(view),
    );
    desktop.add_window(window, screen);

    let result = desktop
        .inspect()
        .invoke(
            screen,
            ComponentTarget::Id("label".to_string()),
            ComponentCommand::Click,
        )
        .expect("fallback invoke");

    assert_eq!(result.dispatch, InvokeDispatch::CoordinateFallback);
}

#[test]
fn wait_for_property_equals_observes_background_binding_change() {
    let screen = Rect::new(0, 0, 80, 24);
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let text = Binding::new("pending".to_string());
    let view = Label::new(text.clone()).tag("status");
    let window = Window::new(
        WindowKind::Normal,
        "Status",
        Rect::new(1, 1, 30, 6),
        Box::new(view),
    );
    desktop.add_window(window, screen);

    let writer = {
        let text = text.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            text.set("ready".to_string());
        })
    };

    let result = desktop
        .inspect()
        .wait_for(
            screen,
            WaitCondition::property_equals(
                ComponentTarget::Id("status".to_string()),
                "text",
                ComponentValue::String("ready".to_string()),
            ),
            Duration::from_secs(1),
        )
        .expect("wait_for");
    writer.join().expect("writer thread");

    assert_eq!(
        result.value,
        Some(ComponentValue::String("ready".to_string()))
    );
    assert!(result.polls >= 1);
}

#[test]
fn wait_for_property_equals_times_out_without_hanging() {
    let screen = Rect::new(0, 0, 80, 24);
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let view = Label::new("pending").tag("status");
    let window = Window::new(
        WindowKind::Normal,
        "Status",
        Rect::new(1, 1, 30, 6),
        Box::new(view),
    );
    desktop.add_window(window, screen);

    let err = desktop
        .inspect()
        .wait_for_with_interval(
            screen,
            WaitCondition::property_equals(
                ComponentTarget::Id("status".to_string()),
                "text",
                ComponentValue::String("never".to_string()),
            ),
            Duration::from_millis(20),
            Duration::from_millis(1),
        )
        .expect_err("wait_for should time out");

    assert!(matches!(err, ComponentError::Timeout(_)));
}

#[test]
fn inspect_can_select_tab() {
    let screen = Rect::new(0, 0, 80, 24);
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let selection = Binding::new(0usize);
    let tabs = TabView::new()
        .selection(selection.clone())
        .tab("One", Label::new("one"))
        .tab("Two", Label::new("two"))
        .tag("tabs");

    let window = Window::new(
        WindowKind::Normal,
        "Tabs",
        Rect::new(1, 1, 30, 8),
        Box::new(tabs),
    );
    desktop.add_window(window, screen);

    let mut inspector = desktop.inspect();
    inspector
        .action(screen, "tabs", ComponentCommand::SelectIndex(1))
        .expect("select");
    assert_eq!(selection.get(), 1);
}

#[test]
fn inspect_can_set_table_rows() {
    let screen = Rect::new(0, 0, 80, 24);
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let rows = Binding::new(vec![vec!["a".into(), "b".into()]]);
    let table = crate::composable::TableView::new(
        "Data",
        vec!["H1".into(), "H2".into()],
        rows.clone(),
        Binding::new(0usize),
    )
    .tag("table");

    let window = Window::new(
        WindowKind::Normal,
        "Table",
        Rect::new(1, 1, 40, 10),
        Box::new(table),
    );
    desktop.add_window(window, screen);

    let mut inspector = desktop.inspect();
    let new_rows = vec![vec!["x".into(), "y".into()], vec!["1".into(), "2".into()]];
    inspector
        .set_property("table", "rows", ComponentValue::Table(new_rows.clone()))
        .expect("rows");
    assert_eq!(rows.get(), new_rows);
}

#[test]
fn inspect_can_toggle_visibility() {
    let screen = Rect::new(0, 0, 80, 24);
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let visible = Binding::new(true);
    let view = Visibility::new(visible.clone(), Label::new("Hello")).tag("vis");
    let window = Window::new(
        WindowKind::Normal,
        "Vis",
        Rect::new(1, 1, 20, 6),
        Box::new(view),
    );
    desktop.add_window(window, screen);

    let mut inspector = desktop.inspect();
    inspector
        .set_property("vis", "visible", ComponentValue::Bool(false))
        .expect("visible");
    assert!(!visible.get());
}
