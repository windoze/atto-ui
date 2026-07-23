use super::tree::{PropertyApply, apply_property_to_view, move_node};
use super::*;
use crate::composable::{
    Component, ComponentContext, ComponentId, EventResult, MouseCoordinateSpace, ScrollbarHost,
    TabMode,
};
use crate::theme::Theme;
use crate::wm::WindowId;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

fn child_tags(view: &dyn Component) -> Vec<Option<&str>> {
    view.children()
        .iter()
        .map(|child| child.view.tag())
        .collect()
}

fn find_view_by_tag<'a>(view: &'a dyn Component, id: &str) -> Option<&'a dyn Component> {
    if view.tag() == Some(id) {
        return Some(view);
    }
    for child in view.children() {
        if let Some(found) = find_view_by_tag(child.view.as_ref(), id) {
            return Some(found);
        }
    }
    None
}

fn child_node_id_by_tag(view: &dyn Component, id: &str) -> Option<ComponentId> {
    view.children()
        .iter()
        .find(|child| child.view.tag() == Some(id))
        .map(|child| child.id)
}

#[test]
fn component_button_click_emits_callback() {
    let callbacks = CallbackRegistry::new();
    let cb = callbacks.register();

    let mut spec = ComponentSpec::new("Button")
        .with_id("btn")
        .with_prop("label", ComponentValue::String("OK".into()));
    spec.events.insert("click".into(), cb);

    let registry = builtin_registry(callbacks.clone());
    let mut view = registry.build(&spec).expect("build");

    let ctx = ComponentContext {
        theme: &Theme::dark(),
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let event = Event::Key(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    let result = view.handle_event(&event, ctx);
    assert_eq!(result, EventResult::submitted());

    let events = callbacks.drain();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].callback_id, cb);
    assert_eq!(events[0].target_id.as_deref(), Some("btn"));
    assert_eq!(events[0].event, "click");
}

#[test]
fn component_checkbox_change_emits_callback() {
    let callbacks = CallbackRegistry::new();
    let cb = callbacks.register();

    let mut spec = ComponentSpec::new("Checkbox")
        .with_id("chk")
        .with_prop("label", ComponentValue::String("A".into()))
        .with_prop("checked", ComponentValue::Bool(false));
    spec.events.insert("change".into(), cb);

    let registry = builtin_registry(callbacks.clone());
    let mut view = registry.build(&spec).expect("build");

    let ctx = ComponentContext {
        theme: &Theme::dark(),
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let event = Event::Key(KeyEvent {
        code: KeyCode::Char(' '),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    let result = view.handle_event(&event, ctx);
    assert_eq!(result, EventResult::changed());

    let events = callbacks.drain();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].callback_id, cb);
    assert_eq!(events[0].target_id.as_deref(), Some("chk"));
    assert_eq!(events[0].event, "change");
    assert_eq!(events[0].payload, Some(ComponentValue::Bool(true)));
}

#[test]
fn component_textbox_change_emits_text_payload() {
    let callbacks = CallbackRegistry::new();
    let cb = callbacks.register();

    let mut spec = ComponentSpec::new("TextBox")
        .with_id("name")
        .with_prop("title", ComponentValue::String("Name".into()))
        .with_prop("text", ComponentValue::String(String::new()));
    spec.events.insert("change".into(), cb);

    let registry = builtin_registry(callbacks.clone());
    let mut view = registry.build(&spec).expect("build");

    let ctx = ComponentContext {
        theme: &Theme::dark(),
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let event = Event::Key(KeyEvent {
        code: KeyCode::Char('A'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    let result = view.handle_event(&event, ctx);
    assert_eq!(result, EventResult::changed());

    let events = callbacks.drain();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].callback_id, cb);
    assert_eq!(events[0].target_id.as_deref(), Some("name"));
    assert_eq!(events[0].event, "change");
    assert_eq!(events[0].payload, Some(ComponentValue::String("A".into())));
}

#[test]
fn component_tree_ops_rebuild_children() {
    let callbacks = CallbackRegistry::new();
    let mut tree =
        ComponentTree::new(ComponentSpec::new("VStack").with_id("root"), callbacks).expect("tree");

    let child = ComponentSpecChild::new(
        ComponentSpec::new("Label")
            .with_id("title")
            .with_prop("text", ComponentValue::String("Hello".into())),
    )
    .with_layout(LayoutSpec {
        width: SizeSpec::Fixed(8),
        height: SizeSpec::Content,
        ..LayoutSpec::default()
    });

    tree.apply_ops(&[TreeOp::Insert {
        parent_id: "root".into(),
        index: 0,
        child,
    }])
    .expect("apply ops");

    tree.rebuild().expect("rebuild");
    let children = tree.view().children();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].layout.width, crate::composable::Size::Fixed(8));
    let value = children[0]
        .view
        .get_property("text")
        .expect("text property");
    assert_eq!(value, ComponentValue::String("Hello".into()));
}

#[test]
fn component_tree_incremental_set_prop_updates_view() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label")
                .with_id("title")
                .with_prop("text", ComponentValue::String("A".into())),
        ));
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    let changed = tree
        .apply_ops_incremental(&[TreeOp::SetProp {
            id: "title".into(),
            name: "text".into(),
            value: ComponentValue::String("B".into()),
        }])
        .expect("apply");
    assert!(!changed);

    let children = tree.view().children();
    let value = children[0]
        .view
        .get_property("text")
        .expect("text property");
    assert_eq!(value, ComponentValue::String("B".into()));
}

#[test]
fn component_tree_incremental_clear_prop_rebuilds_view_with_default() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label")
                .with_id("title")
                .with_prop("text", ComponentValue::String("A".into())),
        ));
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    let changed = tree
        .apply_ops_incremental(&[TreeOp::ClearProp {
            id: "title".into(),
            name: "text".into(),
        }])
        .expect("apply");
    assert!(changed);

    let spec_child = tree
        .root_spec()
        .children
        .iter()
        .find(|child| child.node.id.as_deref() == Some("title"))
        .expect("title spec");
    assert_eq!(spec_child.node.props.get("text"), None);
    let title = find_view_by_tag(tree.view(), "title").expect("title view");
    assert_eq!(
        title.get_property("text"),
        Some(ComponentValue::String(String::new()))
    );
}

#[test]
fn apply_property_distinguishes_unsupported_property_from_missing_node() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label")
                .with_id("title")
                .with_prop("text", ComponentValue::String("A".into())),
        ));
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    let unsupported = apply_property_to_view(
        tree.view_mut(),
        "title",
        "missing_prop",
        &ComponentValue::String("B".into()),
    )
    .expect("apply unsupported");
    assert_eq!(unsupported, PropertyApply::UnsupportedProperty);

    let missing = apply_property_to_view(
        tree.view_mut(),
        "missing_node",
        "text",
        &ComponentValue::String("B".into()),
    )
    .expect("apply missing");
    assert_eq!(missing, PropertyApply::NotFound);
}

#[test]
fn component_tree_incremental_uses_current_root_for_local_replace() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("VStack").with_id("container"),
        ));
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    let child = ComponentSpecChild::new(
        ComponentSpec::new("Label")
            .with_id("inserted")
            .with_prop("text", ComponentValue::String("Hello".into())),
    );

    let changed = tree
        .apply_ops_incremental(&[
            TreeOp::SetProp {
                id: "container".into(),
                name: "unknown".into(),
                value: ComponentValue::Bool(true),
            },
            TreeOp::Insert {
                parent_id: "container".into(),
                index: 0,
                child,
            },
        ])
        .expect("apply");
    assert!(changed);

    let container = find_view_by_tag(tree.view(), "container").expect("container");
    assert_eq!(child_tags(container), vec![Some("inserted")]);
    let root_container = tree
        .root_spec()
        .children
        .iter()
        .find(|child| child.node.id.as_deref() == Some("container"))
        .expect("container spec");
    assert_eq!(root_container.node.children.len(), 1);
}

#[test]
fn component_tree_incremental_insert_child() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack").with_id("root");
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    let child = ComponentSpecChild::new(
        ComponentSpec::new("Label")
            .with_id("a")
            .with_prop("text", ComponentValue::String("Hello".into())),
    )
    .with_layout(LayoutSpec {
        width: SizeSpec::Fixed(5),
        height: SizeSpec::Content,
        ..LayoutSpec::default()
    });

    let changed = tree
        .apply_ops_incremental(&[TreeOp::Insert {
            parent_id: "root".into(),
            index: 0,
            child,
        }])
        .expect("apply");
    assert!(changed);

    let children = tree.view().children();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].layout.width, crate::composable::Size::Fixed(5));
    let value = children[0]
        .view
        .get_property("text")
        .expect("text property");
    assert_eq!(value, ComponentValue::String("Hello".into()));
}

#[test]
fn component_tree_incremental_insert_before_appends_and_uses_anchor_without_rebuild() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("a"),
        ))
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("b"),
        ));
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");
    let b_node_id = child_node_id_by_tag(tree.view(), "b").expect("b node id");

    let changed = tree
        .apply_ops_incremental(&[TreeOp::InsertBefore {
            parent_id: "root".into(),
            anchor_id: Some("b".into()),
            child: ComponentSpecChild::new(ComponentSpec::new("Label").with_id("x")),
        }])
        .expect("insert before anchor");
    assert!(changed);
    assert_eq!(
        child_tags(tree.view()),
        vec![Some("a"), Some("x"), Some("b")]
    );
    assert_eq!(child_node_id_by_tag(tree.view(), "b"), Some(b_node_id));

    let changed = tree
        .apply_ops_incremental(&[TreeOp::InsertBefore {
            parent_id: "root".into(),
            anchor_id: None,
            child: ComponentSpecChild::new(ComponentSpec::new("Label").with_id("tail")),
        }])
        .expect("append without anchor");
    assert!(changed);
    assert_eq!(
        child_tags(tree.view()),
        vec![Some("a"), Some("x"), Some("b"), Some("tail")]
    );
    assert_eq!(child_node_id_by_tag(tree.view(), "b"), Some(b_node_id));
}

#[test]
fn component_tree_incremental_insert_before_existing_child_moves_without_rebuild() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("a"),
        ))
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("b"),
        ))
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("c"),
        ));
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");
    let a_node_id = child_node_id_by_tag(tree.view(), "a").expect("a node id");

    let changed = tree
        .apply_ops_incremental(&[TreeOp::InsertBefore {
            parent_id: "root".into(),
            anchor_id: Some("c".into()),
            child: ComponentSpecChild::new(ComponentSpec::new("Label").with_id("a")),
        }])
        .expect("move before anchor");

    assert!(changed);
    assert_eq!(
        child_tags(tree.view()),
        vec![Some("b"), Some("a"), Some("c")]
    );
    assert_eq!(child_node_id_by_tag(tree.view(), "a"), Some(a_node_id));
    let ids: Vec<Option<&str>> = tree
        .root_spec()
        .children
        .iter()
        .map(|child| child.node.id.as_deref())
        .collect();
    assert_eq!(ids, vec![Some("b"), Some("a"), Some("c")]);
}

#[test]
fn component_tree_incremental_remove_child() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("a"),
        ))
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("b"),
        ));
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    let changed = tree
        .apply_ops_incremental(&[TreeOp::Remove { id: "a".into() }])
        .expect("apply");
    assert!(changed);

    let children = tree.view().children();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].view.tag(), Some("b"));
}

#[test]
fn component_tree_incremental_move_child() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("a"),
        ))
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("b"),
        ))
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("c"),
        ));
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    let changed = tree
        .apply_ops_incremental(&[TreeOp::Move {
            id: "c".into(),
            new_parent_id: "root".into(),
            index: 0,
        }])
        .expect("apply");
    assert!(changed);

    let children = tree.view().children();
    let ids: Vec<Option<&str>> = children.iter().map(|child| child.view.tag()).collect();
    assert_eq!(ids, vec![Some("c"), Some("a"), Some("b")]);
}

#[test]
fn component_tree_incremental_view_index_tracks_shifted_paths_within_batch() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label")
                .with_id("a")
                .with_prop("text", ComponentValue::String("A".into())),
        ))
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label")
                .with_id("b")
                .with_prop("text", ComponentValue::String("B".into())),
        ))
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label")
                .with_id("c")
                .with_prop("text", ComponentValue::String("C".into())),
        ));
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    let changed = tree
        .apply_ops_incremental(&[
            TreeOp::Insert {
                parent_id: "root".into(),
                index: 0,
                child: ComponentSpecChild::new(
                    ComponentSpec::new("Label")
                        .with_id("x")
                        .with_prop("text", ComponentValue::String("X".into())),
                ),
            },
            TreeOp::Remove { id: "a".into() },
            TreeOp::SetProp {
                id: "c".into(),
                name: "text".into(),
                value: ComponentValue::String("C2".into()),
            },
            TreeOp::Replace {
                id: "b".into(),
                node: ComponentSpecChild::new(
                    ComponentSpec::new("Button")
                        .with_id("b")
                        .with_prop("label", ComponentValue::String("B2".into())),
                ),
            },
            TreeOp::Move {
                id: "c".into(),
                new_parent_id: "root".into(),
                index: 1,
            },
        ])
        .expect("apply");
    assert!(changed);

    let children = tree.view().children();
    let ids: Vec<Option<&str>> = children.iter().map(|child| child.view.tag()).collect();
    assert_eq!(ids, vec![Some("x"), Some("c"), Some("b")]);
    assert_eq!(
        children[1].view.get_property("text"),
        Some(ComponentValue::String("C2".into()))
    );
    assert!(children[2].view.type_name().ends_with("Button"));
    assert_eq!(
        children[2].view.get_property("label"),
        Some(ComponentValue::String("B2".into()))
    );
}

#[test]
fn component_tree_incremental_visibility_bind_event_matches_rebuild_shape() {
    let callbacks = CallbackRegistry::new();
    let cb = callbacks.register();
    let root = ComponentSpec::new("Visibility")
        .with_id("vis")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("VStack")
                .with_id("inner")
                .with_child(ComponentSpecChild::new(
                    ComponentSpec::new("Button")
                        .with_id("leaf")
                        .with_prop("label", ComponentValue::String("Go".into())),
                )),
        ));
    let mut tree = ComponentTree::new(root, callbacks.clone()).expect("tree");

    assert_eq!(child_tags(tree.view()), vec![Some("leaf")]);

    let changed = tree
        .apply_ops_incremental(&[TreeOp::BindEvent {
            id: "leaf".into(),
            event: "click".into(),
            callback: cb,
        }])
        .expect("bind");

    assert!(changed);
    assert_eq!(child_tags(tree.view()), vec![Some("leaf")]);

    let rebuilt = ComponentTree::new(tree.root_spec().clone(), callbacks).expect("rebuilt");
    assert_eq!(child_tags(tree.view()), child_tags(rebuilt.view()));
}

#[test]
fn component_tree_incremental_unknown_insert_rolls_back_root_and_view() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack").with_id("root");
    let original = root.clone();
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    let err = tree
        .apply_ops_incremental(&[TreeOp::Insert {
            parent_id: "root".into(),
            index: 0,
            child: ComponentSpecChild::new(ComponentSpec::new("MissingWidget").with_id("bad")),
        }])
        .expect_err("unknown component should fail");

    assert_eq!(err, TreeError::UnknownComponent("MissingWidget".into()));
    assert_eq!(tree.root_spec(), &original);
    assert!(tree.view().children().is_empty());
}

#[test]
fn component_tree_incremental_batch_failure_rolls_back_partial_view_update() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack").with_id("root");
    let original = root.clone();
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    let err = tree
        .apply_ops_incremental(&[
            TreeOp::Insert {
                parent_id: "root".into(),
                index: 0,
                child: ComponentSpecChild::new(
                    ComponentSpec::new("Label")
                        .with_id("ok")
                        .with_prop("text", ComponentValue::String("OK".into())),
                ),
            },
            TreeOp::Insert {
                parent_id: "root".into(),
                index: 1,
                child: ComponentSpecChild::new(ComponentSpec::new("MissingWidget").with_id("bad")),
            },
        ])
        .expect_err("second insert should fail");

    assert_eq!(err, TreeError::UnknownComponent("MissingWidget".into()));
    assert_eq!(tree.root_spec(), &original);
    assert!(tree.view().children().is_empty());
}

#[test]
fn component_tree_incremental_invalid_set_prop_rolls_back_root_and_view() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label")
                .with_id("title")
                .with_prop("text", ComponentValue::String("A".into())),
        ));
    let original = root.clone();
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    let err = tree
        .apply_ops_incremental(&[TreeOp::SetProp {
            id: "title".into(),
            name: "text".into(),
            value: ComponentValue::Bool(true),
        }])
        .expect_err("invalid property value should fail");

    assert!(matches!(err, TreeError::InvalidProperty { .. }));
    assert_eq!(tree.root_spec(), &original);
    let children = tree.view().children();
    assert_eq!(
        children[0].view.get_property("text"),
        Some(ComponentValue::String("A".into()))
    );
}

#[test]
fn component_tree_apply_ops_and_rebuild_failure_preserves_root_and_view() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack").with_id("root");
    let original = root.clone();
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    let err = tree
        .apply_ops_and_rebuild(&[TreeOp::Insert {
            parent_id: "root".into(),
            index: 0,
            child: ComponentSpecChild::new(ComponentSpec::new("MissingWidget").with_id("bad")),
        }])
        .expect_err("unknown component should fail");

    assert_eq!(err, TreeError::UnknownComponent("MissingWidget".into()));
    assert_eq!(tree.root_spec(), &original);
    assert!(tree.view().children().is_empty());
}

#[test]
fn component_tree_incremental_move_missing_parent_preserves_root_and_view() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("a"),
        ))
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("VStack")
                .with_id("container")
                .with_child(ComponentSpecChild::new(
                    ComponentSpec::new("Label").with_id("b"),
                )),
        ));
    let original = root.clone();
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    let err = tree
        .apply_ops_incremental(&[TreeOp::Move {
            id: "a".into(),
            new_parent_id: "missing".into(),
            index: 0,
        }])
        .expect_err("missing parent should fail");

    assert_eq!(err, TreeError::NotFound("missing".into()));
    assert_eq!(tree.root_spec(), &original);
    assert_eq!(child_tags(tree.view()), vec![Some("a"), Some("container")]);
    let container = find_view_by_tag(tree.view(), "container").expect("container");
    assert_eq!(child_tags(container), vec![Some("b")]);
}

#[test]
fn move_node_missing_parent_keeps_node_in_place() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("a"),
        ))
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("VStack")
                .with_id("container")
                .with_child(ComponentSpecChild::new(
                    ComponentSpec::new("Label").with_id("b"),
                )),
        ));
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    assert!(!move_node(tree.view_mut(), "a", "missing", 0));

    assert_eq!(child_tags(tree.view()), vec![Some("a"), Some("container")]);
    let container = find_view_by_tag(tree.view(), "container").expect("container");
    assert_eq!(child_tags(container), vec![Some("b")]);
}

#[test]
fn move_node_tab_view_parent_keeps_node_in_place() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("a"),
        ))
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("TabView")
                .with_id("tabs")
                .with_child(ComponentSpecChild::new(
                    ComponentSpec::new("Label").with_id("tab-child"),
                )),
        ));
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    assert!(!move_node(tree.view_mut(), "a", "tabs", 0));

    assert_eq!(child_tags(tree.view()), vec![Some("a"), Some("tabs")]);
    let tabs = find_view_by_tag(tree.view(), "tabs").expect("tabs");
    assert_eq!(child_tags(tabs), vec![Some("tab-child")]);
}

#[test]
fn move_node_leaf_parent_restores_taken_node() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("a"),
        ))
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("leaf"),
        ));
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    assert!(!move_node(tree.view_mut(), "a", "leaf", 0));

    assert_eq!(child_tags(tree.view()), vec![Some("a"), Some("leaf")]);
}

#[test]
fn move_node_normal_move_inserts_at_target_index() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("a"),
        ))
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("VStack")
                .with_id("dest")
                .with_child(ComponentSpecChild::new(
                    ComponentSpec::new("Label").with_id("b"),
                )),
        ))
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("c"),
        ));
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    assert!(move_node(tree.view_mut(), "c", "dest", 0));

    assert_eq!(child_tags(tree.view()), vec![Some("a"), Some("dest")]);
    let dest = find_view_by_tag(tree.view(), "dest").expect("dest");
    assert_eq!(child_tags(dest), vec![Some("c"), Some("b")]);
}

#[test]
fn component_tree_incremental_replace_child() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Label").with_id("a"),
        ));
    let mut tree = ComponentTree::new(root, callbacks).expect("tree");

    let node = ComponentSpecChild::new(
        ComponentSpec::new("Button")
            .with_id("a")
            .with_prop("label", ComponentValue::String("OK".into())),
    )
    .with_layout(LayoutSpec {
        width: SizeSpec::Fixed(6),
        height: SizeSpec::Content,
        ..LayoutSpec::default()
    });

    let changed = tree
        .apply_ops_incremental(&[TreeOp::Replace {
            id: "a".into(),
            node,
        }])
        .expect("apply");
    assert!(changed);

    let children = tree.view().children();
    assert_eq!(children[0].layout.width, crate::composable::Size::Fixed(6));
    assert!(children[0].view.type_name().ends_with("Button"));
    let value = children[0]
        .view
        .get_property("label")
        .expect("label property");
    assert_eq!(value, ComponentValue::String("OK".into()));
}

#[test]
fn component_tree_incremental_bind_and_clear_event() {
    let callbacks = CallbackRegistry::new();
    let cb = callbacks.register();
    let root = ComponentSpec::new("VStack")
        .with_id("root")
        .with_child(ComponentSpecChild::new(
            ComponentSpec::new("Button")
                .with_id("btn")
                .with_prop("label", ComponentValue::String("Go".into())),
        ));
    let mut tree = ComponentTree::new(root, callbacks.clone()).expect("tree");

    let changed = tree
        .apply_ops_incremental(&[TreeOp::BindEvent {
            id: "btn".into(),
            event: "click".into(),
            callback: cb,
        }])
        .expect("bind");
    assert!(changed);

    let ctx = ComponentContext {
        theme: &Theme::dark(),
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let event = Event::Key(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    {
        let children = tree.view_mut().children_mut().expect("children");
        let result = children[0].view.handle_event(&event, ctx);
        assert_eq!(result, EventResult::submitted());
    }
    let events = callbacks.drain();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].callback_id, cb);

    tree.apply_ops_incremental(&[TreeOp::ClearEvent {
        id: "btn".into(),
        event: "click".into(),
    }])
    .expect("clear");

    {
        let children = tree.view_mut().children_mut().expect("children");
        let result = children[0].view.handle_event(&event, ctx);
        assert_eq!(result, EventResult::submitted());
    }
    let events = callbacks.drain();
    assert!(events.is_empty());
}

#[test]
fn builtin_schema_includes_button_props() {
    let registry = builtin_registry(CallbackRegistry::new());
    let schema = registry.schema("Button").expect("schema");
    assert!(schema.properties.iter().any(|prop| prop.name == "label"));
    assert!(schema.events.iter().any(|event| event.name == "click"));
}

#[test]
fn builtin_schema_includes_stack_padding() {
    let registry = builtin_registry(CallbackRegistry::new());
    let schema = registry.schema("VStack").expect("schema");
    let padding = schema
        .properties
        .iter()
        .find(|prop| prop.name == "padding")
        .expect("padding");
    assert_eq!(padding.value_type, ValueType::Map);
}

#[test]
fn builtin_schema_includes_styled_label_link_event() {
    let registry = builtin_registry(CallbackRegistry::new());
    let schema = registry.schema("StyledLabel").expect("schema");
    let link = schema
        .events
        .iter()
        .find(|event| event.name == "link")
        .expect("link");
    assert_eq!(link.payload, Some(ValueType::String));
}

#[test]
fn builtin_schema_includes_rich_text_and_text_span() {
    let registry = builtin_registry(CallbackRegistry::new());
    let rich = registry.schema("RichText").expect("rich text schema");
    assert!(rich.allows_children);
    let link = rich
        .events
        .iter()
        .find(|event| event.name == "link")
        .expect("link");
    assert_eq!(link.payload, Some(ValueType::String));

    let span = registry.schema("TextSpan").expect("text span schema");
    assert!(!span.allows_children);
    for prop in [
        "text",
        "bold",
        "italic",
        "underline",
        "strike",
        "color",
        "href",
    ] {
        assert!(
            span.properties.iter().any(|meta| meta.name == prop),
            "missing TextSpan property {prop}"
        );
    }
}

// --- B4 reconcile: props edited only in the view survive a fallback rebuild ---

#[test]
fn component_tree_reconciles_view_edits_into_root_before_rebuild() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("VStack").with_id("root").with_child(
        ComponentSpecChild::new(
            ComponentSpec::new("TextBox")
                .with_id("name")
                .with_prop("title", ComponentValue::String("Name".into()))
                .with_prop("text", ComponentValue::String("old".into())),
        ),
    );
    let mut tree = ComponentTree::new(root, callbacks.clone()).expect("tree");

    // Simulate user input: mutate the view only, never `root`. This mirrors keyboard input landing
    // in the widget's Binding — `set_property`/`apply_command` forward to the view and leave `root`
    // untouched, which is exactly the divergence B4 describes.
    {
        let child = tree
            .view_mut()
            .children_mut()
            .and_then(|children| children.first_mut())
            .expect("child node");
        child
            .view
            .set_property("text", ComponentValue::String("typed".into()))
            .expect("set view text");
    }

    // Sanity: the divergence exists — view has the edit, root still has the stale value.
    assert_eq!(
        find_view_by_tag(tree.view(), "name").and_then(|v| v.get_property("text")),
        Some(ComponentValue::String("typed".into()))
    );
    assert_eq!(
        tree.root_spec().children[0].node.props.get("text"),
        Some(&ComponentValue::String("old".into())),
        "root should still hold the stale declared value before reconcile"
    );

    // A ClearProp on an unrelated declared prop rebuilds the affected subtree from spec, which is
    // the fallback path that would drop view-only edits without reconcile.
    let changed = tree
        .apply_ops_incremental(&[TreeOp::ClearProp {
            id: "name".into(),
            name: "title".into(),
        }])
        .expect("apply");
    assert!(changed);

    // After the rebuild, the user's edit must NOT have been dropped: reconcile wrote it into root
    // before the subtree was rebuilt from spec.
    let name = find_view_by_tag(tree.view(), "name").expect("name view");
    assert_eq!(
        name.get_property("text"),
        Some(ComponentValue::String("typed".into())),
        "user input held only in the view was lost across a rebuild"
    );
}

// --- D3: build-time prop_edge_insets and the codec share one conversion ---

#[test]
fn prop_edge_insets_matches_codec_for_all_input_shapes() {
    use crate::ComponentValueCodec;
    use crate::composable::EdgeInsets;
    use std::collections::BTreeMap;

    let scalar = ComponentValue::U64(3);
    let list = ComponentValue::List(vec![
        ComponentValue::U64(1),
        ComponentValue::U64(2),
        ComponentValue::U64(3),
        ComponentValue::U64(4),
    ]);
    let map = ComponentValue::Map(BTreeMap::from([
        ("top".to_string(), ComponentValue::U64(5)),
        ("right".to_string(), ComponentValue::U64(6)),
        ("bottom".to_string(), ComponentValue::U64(7)),
        ("left".to_string(), ComponentValue::U64(8)),
    ]));

    // Expected results pin the conversion so a break in the shared codec is caught (agreement
    // alone would not: both paths route through the same codec and would break together).
    let expected = [
        (scalar, EdgeInsets::all(3)),
        (
            list,
            EdgeInsets {
                top: 1,
                right: 2,
                bottom: 3,
                left: 4,
            },
        ),
        (
            map,
            EdgeInsets {
                top: 5,
                right: 6,
                bottom: 7,
                left: 8,
            },
        ),
    ];

    for (value, want) in expected {
        let spec = ComponentSpec::new("VStack").with_prop("padding", value.clone());
        let via_build = super::props::prop_edge_insets(&spec, "padding")
            .expect("prop_edge_insets")
            .expect("present");
        let via_codec =
            EdgeInsets::from_component_value(value.clone(), "padding").expect("codec");
        assert_eq!(
            via_build, via_codec,
            "build and codec must agree for input {value:?}"
        );
        assert_eq!(via_build, want, "unexpected conversion for input {value:?}");
    }
}
