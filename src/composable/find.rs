use super::component::Component;

/// Finds the first component tagged with `tag` in a component tree.
///
/// The search checks `view` before recursively walking child nodes in depth-first
/// order, so duplicate tags resolve to the first pre-order match.
pub fn find_by_tag<'a>(view: &'a dyn Component, tag: &str) -> Option<&'a dyn Component> {
    if view.tag() == Some(tag) {
        return Some(view);
    }

    for child in view.children() {
        if let Some(found) = find_by_tag(child.view.as_ref(), tag) {
            return Some(found);
        }
    }

    None
}

/// Finds the first mutable component tagged with `tag` in a component tree.
///
/// This follows the same root-first depth-first order as [`find_by_tag`].
pub fn find_by_tag_mut<'a>(
    view: &'a mut dyn Component,
    tag: &str,
) -> Option<&'a mut dyn Component> {
    if view.tag() == Some(tag) {
        return Some(view);
    }

    let children = view.children_mut()?;
    for child in children {
        if let Some(found) = find_by_tag_mut(child.view.as_mut(), tag) {
            return Some(found);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::*;
    use crate::ComponentValue;
    use crate::composable::{
        ComponentContext, ComponentTagExt, DynamicTree, HStack, Label, VStack,
    };

    struct TestLeaf {
        tag: &'static str,
        label: &'static str,
    }

    impl TestLeaf {
        fn new(tag: &'static str, label: &'static str) -> Self {
            Self { tag, label }
        }
    }

    impl Component for TestLeaf {
        fn get_property(&self, name: &str) -> Option<ComponentValue> {
            (name == "label").then(|| ComponentValue::String(self.label.to_string()))
        }

        fn draw(
            &mut self,
            _frame: &mut ratatui::Frame<'_>,
            _area: Rect,
            _ctx: ComponentContext<'_>,
        ) {
        }
    }

    impl DynamicTree for TestLeaf {
        fn tag(&self) -> Option<&str> {
            Some(self.tag)
        }
    }

    crate::impl_component_default_traits!(TestLeaf => Layout, Scrollable, FocusNav, EventHandling);

    #[test]
    fn find_by_tag_matches_root_node() {
        let view = TestLeaf::new("root", "root");

        let found = find_by_tag(&view, "root").expect("root tag");

        assert_eq!(found.tag(), Some("root"));
    }

    #[test]
    fn find_by_tag_matches_deep_nested_child() {
        let view = VStack::new().child(
            HStack::new()
                .child(Label::new("first").tag("first"))
                .child(Label::new("deep").tag("deep")),
        );

        let found = find_by_tag(&view, "deep").expect("deep tag");

        assert_eq!(found.tag(), Some("deep"));
    }

    #[test]
    fn find_by_tag_returns_none_when_missing() {
        let view = VStack::new().child(Label::new("only").tag("only"));

        assert!(find_by_tag(&view, "missing").is_none());
    }

    #[test]
    fn find_by_tag_returns_first_duplicate_in_dfs_order() {
        let view = VStack::new()
            .child(TestLeaf::new("duplicate", "first"))
            .child(
                HStack::new()
                    .child(TestLeaf::new("duplicate", "second"))
                    .child(TestLeaf::new("other", "third")),
            );

        let found = find_by_tag(&view, "duplicate").expect("duplicate tag");

        assert_eq!(
            found.get_property("label"),
            Some(ComponentValue::String("first".to_string()))
        );
    }

    #[test]
    fn find_by_tag_mut_returns_first_duplicate_in_dfs_order() {
        let mut view = VStack::new()
            .child(TestLeaf::new("duplicate", "first"))
            .child(TestLeaf::new("duplicate", "second"));

        let found = find_by_tag_mut(&mut view, "duplicate").expect("duplicate tag");

        assert_eq!(
            found.get_property("label"),
            Some(ComponentValue::String("first".to_string()))
        );
    }
}
