use crate::ComponentPropertySchema;
use crate::composable::{
    Border, CommandPalette, Component, ComponentTag, Disclosure, DisclosureStatus, Divider,
    DividerOrientation, EdgeInsets, Grid, HStack, Label, LayoutParams, Spacer, Splitter,
    SplitterOrientation, TabView, Text, TextArea, TextBox, TypeAhead, VStack, Visibility,
};
use crate::reactive::Binding;
use crate::widgets::{
    Button, Checkbox, ListBox, ProgressBar, RadioGroup, RichText, Slider, Spinner, StyledLabel,
    TabHeaderPosition, TableView, TextSpan,
};

use super::props::{
    invalid_prop_reason, layout_from_spec, prop_bool, prop_edge_insets, prop_f64, prop_string,
    prop_table, prop_u16, prop_usize, prop_vec_string,
};
use super::{
    ActionMeta, CallbackHandle, CallbackId, CallbackRegistry, ComponentRegistry, ComponentSchema,
    ComponentSpec, EventMeta, TreeError, ValueType,
};

pub fn builtin_registry(callbacks: CallbackRegistry) -> ComponentRegistry<Box<dyn Component>> {
    let mut registry = ComponentRegistry::new();

    register_button(&mut registry, callbacks.clone());
    register_checkbox(&mut registry, callbacks.clone());
    register_disclosure(&mut registry, callbacks.clone());
    register_label(&mut registry);
    register_styled_label(&mut registry, callbacks.clone());
    register_text_span(&mut registry);
    register_rich_text(&mut registry, callbacks.clone());
    register_text(&mut registry);
    register_textbox(&mut registry, callbacks.clone());
    register_textarea(&mut registry, callbacks.clone());
    register_typeahead(&mut registry, callbacks.clone());
    register_command_palette(&mut registry, callbacks.clone());
    register_slider(&mut registry, callbacks.clone());
    register_progress_bar(&mut registry);
    register_radio_group(&mut registry, callbacks.clone());
    register_list_box(&mut registry, callbacks.clone());
    register_table_view(&mut registry, callbacks.clone());
    register_spinner(&mut registry);
    register_tab_view(&mut registry, callbacks.clone());
    register_stack::<VStack>(&mut registry, "VStack", StackAxis::Vertical);
    register_stack::<HStack>(&mut registry, "HStack", StackAxis::Horizontal);
    register_grid(&mut registry);
    register_splitter(&mut registry);
    register_divider(&mut registry);
    register_spacer(&mut registry);
    register_border(&mut registry);
    register_visibility(&mut registry);

    registry
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StackAxis {
    Vertical,
    Horizontal,
}

pub fn component_schema<T: ComponentPropertySchema>(type_name: &str) -> ComponentSchema {
    let mut schema = ComponentSchema::new(type_name).with_properties(T::property_schema());
    schema.dedup_properties();
    schema
}

fn register_button(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<Button>("Button")
        .with_event(EventMeta::new("click"))
        .with_action(ActionMeta::new("click"))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let label = prop_string(spec, "label")?.unwrap_or_else(|| "Button".to_string());
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let mut button = Button::new(label).enabled(enabled);
        if let Some(cb) = event_handle(spec, "click", callbacks.clone()) {
            button = button.on_click_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(button)))
    });
}

fn register_checkbox(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<Checkbox>("Checkbox")
        .with_event(EventMeta::new("change").with_payload(ValueType::Bool))
        .with_action(ActionMeta::new("toggle"))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let label = prop_string(spec, "label")?.unwrap_or_default();
        let checked = prop_bool(spec, "checked")?.unwrap_or(false);
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let mut checkbox = Checkbox::new(label, Binding::new(checked)).enabled(enabled);
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            checkbox = checkbox.on_change_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(checkbox)))
    });
}

fn register_disclosure(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<Disclosure>("Disclosure")
        .with_event(EventMeta::new("toggle"))
        .with_action(ActionMeta::new("toggle"))
        .allow_children(true);

    registry.register(schema, move |spec, registry| {
        let title = prop_string(spec, "title")?.unwrap_or_default();
        let expanded = prop_bool(spec, "expanded")?.unwrap_or(false);
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let content = prop_string(spec, "content")?;
        let status = prop_string(spec, "status")?
            .and_then(|value| DisclosureStatus::parse(&value))
            .unwrap_or_default();

        let mut disclosure = Disclosure::new(title)
            .expanded(expanded)
            .enabled(enabled)
            .status(status);
        if let Some(content) = content {
            disclosure = disclosure.content(content);
        }
        if let Some(cb) = event_handle(spec, "toggle", callbacks.clone()) {
            disclosure = disclosure.on_toggle_callback(cb);
        }

        if spec.children.len() == 1 {
            let child = registry.build(&spec.children[0].node)?;
            disclosure = disclosure.boxed_child(child);
        } else if !spec.children.is_empty() {
            let mut stack = VStack::new();
            for child in &spec.children {
                let view = registry.build(&child.node)?;
                let layout = child
                    .layout
                    .as_ref()
                    .map(layout_from_spec)
                    .unwrap_or_default();
                stack.add_child_with_layout(view, layout);
            }
            disclosure = disclosure.child(stack);
        }

        Ok(wrap_with_id(spec, Box::new(disclosure)))
    });
}

fn register_label(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Label>("Label").allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let text = prop_string(spec, "text")?.unwrap_or_default();
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let label = Label::new(text).enabled(enabled);
        Ok(wrap_with_id(spec, Box::new(label)))
    });
}

fn register_styled_label(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<StyledLabel>("StyledLabel")
        .with_event(EventMeta::new("link").with_payload(ValueType::String))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let text = prop_string(spec, "text")?.unwrap_or_default();
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let mut label = StyledLabel::new(text).enabled(enabled);
        if let Some(cb) = event_handle(spec, "link", callbacks.clone()) {
            label = label.on_link_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(label)))
    });
}

fn register_text_span(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<TextSpan>("TextSpan").allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let text = prop_string(spec, "text")?.unwrap_or_default();
        let mut span = TextSpan::new(text)
            .bold(prop_bool(spec, "bold")?.unwrap_or(false))
            .italic(prop_bool(spec, "italic")?.unwrap_or(false))
            .underline(prop_bool(spec, "underline")?.unwrap_or(false))
            .strike(prop_bool(spec, "strike")?.unwrap_or(false));

        if let Some(color) = prop_string(spec, "color")? {
            span = span
                .color_name(color)
                .map_err(|err| invalid_prop_reason(spec, "color", err))?;
        }
        if let Some(href) = prop_string(spec, "href")? {
            span = span.href(href);
        }

        Ok(wrap_with_id(spec, Box::new(span)))
    });
}

fn register_rich_text(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<RichText>("RichText")
        .with_event(EventMeta::new("link").with_payload(ValueType::String))
        .allow_children(true);

    registry.register(schema, move |spec, registry| {
        let mut rich_text = RichText::new();
        if let Some(cb) = event_handle(spec, "link", callbacks.clone()) {
            rich_text = rich_text.on_link_callback(cb);
        }

        for child in &spec.children {
            if child.node.type_name != "TextSpan" {
                return Err(TreeError::InvalidTreeOp(
                    "RichText only accepts TextSpan children".to_string(),
                ));
            }
            let view = registry.build(&child.node)?;
            let layout = child
                .layout
                .as_ref()
                .map(layout_from_spec)
                .unwrap_or_default();
            rich_text.add_child_with_layout(view, layout);
        }

        Ok(wrap_with_id(spec, Box::new(rich_text)))
    });
}

fn register_text(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Text>("Text").allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let text = prop_string(spec, "text")?.unwrap_or_default();
        let selectable = prop_bool(spec, "selectable")?.unwrap_or(false);
        let clipboard = prop_string(spec, "clipboard")?.unwrap_or_default();
        let view = Text::new(text).selectable(selectable).clipboard(clipboard);
        Ok(wrap_with_id(spec, Box::new(view)))
    });
}

fn register_textbox(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<TextBox>("TextBox")
        .with_event(EventMeta::new("change").with_payload(ValueType::String))
        .with_event(EventMeta::new("submit"))
        .with_action(ActionMeta::new("input_text").with_payload(ValueType::String))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let title = prop_string(spec, "title")?.unwrap_or_default();
        let text = prop_string(spec, "text")?.unwrap_or_default();
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let clipboard = prop_string(spec, "clipboard")?.unwrap_or_default();
        let placeholder = prop_string(spec, "placeholder")?;

        let mut textbox = TextBox::new(title, Binding::new(text))
            .enabled(enabled)
            .clipboard(clipboard);
        if let Some(value) = placeholder {
            textbox = textbox.placeholder(value);
        }
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            textbox = textbox.on_change_callback(cb);
        }
        if let Some(cb) = event_handle(spec, "submit", callbacks.clone()) {
            textbox = textbox.on_submit_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(textbox)))
    });
}

fn register_textarea(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<TextArea>("TextArea")
        .with_event(EventMeta::new("change").with_payload(ValueType::String))
        .with_event(EventMeta::new("submit"))
        .with_action(ActionMeta::new("input_text").with_payload(ValueType::String))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let title = prop_string(spec, "title")?.unwrap_or_default();
        let text = prop_string(spec, "text")?.unwrap_or_default();
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let clipboard = prop_string(spec, "clipboard")?.unwrap_or_default();
        let kill_ring = prop_string(spec, "kill_ring")?.unwrap_or_default();
        let history = prop_vec_string(spec, "history")?.unwrap_or_default();
        let height = prop_u16(spec, "height")?.unwrap_or(5);
        let enter_submits = prop_bool(spec, "enter_submits")?.unwrap_or(false);
        let placeholder = prop_string(spec, "placeholder")?;

        let mut textarea = TextArea::new(title, Binding::new(text))
            .enabled(enabled)
            .clipboard(clipboard)
            .kill_ring(kill_ring)
            .history(Binding::new(history))
            .height(height)
            .enter_submits(enter_submits);
        if let Some(value) = placeholder {
            textarea = textarea.placeholder(value);
        }
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            textarea = textarea.on_change_callback(cb);
        }
        if let Some(cb) = event_handle(spec, "submit", callbacks.clone()) {
            textarea = textarea.on_submit_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(textarea)))
    });
}

fn register_typeahead(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<TypeAhead>("TypeAhead")
        .with_event(EventMeta::new("change").with_payload(ValueType::String))
        .with_event(EventMeta::new("accept").with_payload(ValueType::String))
        .with_event(EventMeta::new("close"))
        .with_action(ActionMeta::new("input_text").with_payload(ValueType::String))
        .with_action(ActionMeta::new("select_index").with_payload(ValueType::U64))
        .with_action(ActionMeta::new("submit"))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let title = prop_string(spec, "title")?.unwrap_or_else(|| "TypeAhead".to_string());
        let query = prop_string(spec, "query")?.unwrap_or_default();
        let items = prop_vec_string(spec, "items")?.unwrap_or_default();
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let selection = prop_usize(spec, "selection")?.unwrap_or(0);
        let accepted = prop_string(spec, "accepted")?.unwrap_or_default();
        let open = prop_bool(spec, "open")?.unwrap_or(false);
        let open_on_empty = prop_bool(spec, "open_on_empty")?.unwrap_or(false);
        let placeholder = prop_string(spec, "placeholder")?;
        let height = prop_u16(spec, "height")?.unwrap_or(8);
        let max_results = prop_usize(spec, "max_results")?.unwrap_or(8);

        let mut view = TypeAhead::new(title, Binding::new(query), Binding::new(items))
            .enabled(enabled)
            .selection(Binding::new(selection))
            .accepted(Binding::new(accepted))
            .open(open)
            .open_on_empty(open_on_empty)
            .height(height)
            .max_results(max_results);
        if let Some(placeholder) = placeholder {
            view = view.placeholder(placeholder);
        }
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            view = view.on_change_callback(cb);
        }
        if let Some(cb) = event_handle(spec, "accept", callbacks.clone()) {
            view = view.on_accept_callback(cb);
        }
        if let Some(cb) = event_handle(spec, "close", callbacks.clone()) {
            view = view.on_close_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(view)))
    });
}

fn register_command_palette(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<CommandPalette>("CommandPalette")
        .with_event(EventMeta::new("change").with_payload(ValueType::String))
        .with_event(EventMeta::new("accept").with_payload(ValueType::String))
        .with_event(EventMeta::new("close"))
        .with_action(ActionMeta::new("input_text").with_payload(ValueType::String))
        .with_action(ActionMeta::new("select_index").with_payload(ValueType::U64))
        .with_action(ActionMeta::new("submit"))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let title = prop_string(spec, "title")?.unwrap_or_else(|| "Command Palette".to_string());
        let query = prop_string(spec, "query")?.unwrap_or_default();
        let items = prop_vec_string(spec, "items")?.unwrap_or_default();
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let selection = prop_usize(spec, "selection")?.unwrap_or(0);
        let accepted = prop_string(spec, "accepted")?.unwrap_or_default();
        let open = prop_bool(spec, "open")?.unwrap_or(true);
        let open_on_empty = prop_bool(spec, "open_on_empty")?.unwrap_or(true);
        let placeholder = prop_string(spec, "placeholder")?;
        let height = prop_u16(spec, "height")?.unwrap_or(8);
        let max_results = prop_usize(spec, "max_results")?.unwrap_or(8);

        let mut view = CommandPalette::new(title, Binding::new(query), Binding::new(items))
            .enabled(enabled)
            .selection(Binding::new(selection))
            .accepted(Binding::new(accepted))
            .open(open)
            .open_on_empty(open_on_empty)
            .height(height)
            .max_results(max_results);
        if let Some(placeholder) = placeholder {
            view = view.placeholder(placeholder);
        }
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            view = view.on_change_callback(cb);
        }
        if let Some(cb) = event_handle(spec, "accept", callbacks.clone()) {
            view = view.on_accept_callback(cb);
        }
        if let Some(cb) = event_handle(spec, "close", callbacks.clone()) {
            view = view.on_close_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(view)))
    });
}

fn register_slider(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<Slider>("Slider")
        .with_event(EventMeta::new("change").with_payload(ValueType::F64))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let min = prop_f64(spec, "min")?.unwrap_or(0.0);
        let max = prop_f64(spec, "max")?.unwrap_or(1.0);
        let value = prop_f64(spec, "value")?.unwrap_or(min);
        let step = prop_f64(spec, "step")?.unwrap_or(1.0);
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let mut slider = Slider::new(min, max, Binding::new(value))
            .step(step)
            .enabled(enabled);
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            slider = slider.on_change_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(slider)))
    });
}

fn register_progress_bar(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<ProgressBar>("ProgressBar").allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let min = prop_f64(spec, "min")?.unwrap_or(0.0);
        let max = prop_f64(spec, "max")?.unwrap_or(1.0);
        let value = prop_f64(spec, "value")?.unwrap_or(min);
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let show_text = prop_bool(spec, "show_text")?.unwrap_or(false);
        let text = prop_string(spec, "text")?;
        let mut bar = ProgressBar::new(min, max, Binding::new(value))
            .enabled(enabled)
            .show_text(show_text);
        if let Some(text) = text {
            bar = bar.text(text);
        }
        Ok(wrap_with_id(spec, Box::new(bar)))
    });
}

fn register_radio_group(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<RadioGroup>("RadioGroup")
        .with_event(EventMeta::new("change").with_payload(ValueType::U64))
        .with_action(ActionMeta::new("select_index").with_payload(ValueType::U64))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let label = prop_string(spec, "label")?.unwrap_or_default();
        let options = prop_vec_string(spec, "options")?.unwrap_or_default();
        let selection = prop_usize(spec, "selection")?.unwrap_or(0);
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let height = prop_u16(spec, "height")?;

        let mut radio =
            RadioGroup::new(label, Binding::new(options), Binding::new(selection)).enabled(enabled);
        if let Some(height) = height {
            radio = radio.height(height);
        }
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            radio = radio.on_change_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(radio)))
    });
}

fn register_list_box(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<ListBox>("ListBox")
        .with_event(EventMeta::new("change").with_payload(ValueType::U64))
        .with_action(ActionMeta::new("select_index").with_payload(ValueType::U64))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let title = prop_string(spec, "title")?.unwrap_or_default();
        let items = prop_vec_string(spec, "items")?.unwrap_or_default();
        let selection = prop_usize(spec, "selection")?.unwrap_or(0);
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let height = prop_u16(spec, "height")?;

        let mut list =
            ListBox::new(title, Binding::new(items), Binding::new(selection)).enabled(enabled);
        if let Some(height) = height {
            list = list.height(height);
        }
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            list = list.on_change_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(list)))
    });
}

fn register_table_view(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<TableView>("TableView")
        .with_event(EventMeta::new("change").with_payload(ValueType::U64))
        .with_action(ActionMeta::new("select_index").with_payload(ValueType::U64))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let title = prop_string(spec, "title")?.unwrap_or_default();
        let headers = prop_vec_string(spec, "headers")?.unwrap_or_default();
        let rows = prop_table(spec, "rows")?.unwrap_or_default();
        let selection = prop_usize(spec, "selection")?.unwrap_or(0);
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let height = prop_u16(spec, "height")?;

        let mut table = TableView::new(
            title,
            Binding::new(headers),
            Binding::new(rows),
            Binding::new(selection),
        )
        .enabled(enabled);
        if let Some(height) = height {
            table = table.height(height);
        }
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            table = table.on_change_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(table)))
    });
}

fn register_spinner(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Spinner>("Spinner").allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let text = prop_string(spec, "text")?.unwrap_or_default();
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let running = prop_bool(spec, "running")?.unwrap_or(true);
        let spinner = Spinner::new(text).enabled(enabled).running(running);
        Ok(wrap_with_id(spec, Box::new(spinner)))
    });
}

fn register_tab_view(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<TabView>("TabView")
        .with_event(EventMeta::new("change").with_payload(ValueType::U64))
        .with_action(ActionMeta::new("select_index").with_payload(ValueType::U64))
        .allow_children(true);

    registry.register(schema, move |spec, registry| {
        let selection = prop_usize(spec, "selection")?.unwrap_or(0);
        let header_position = prop_string(spec, "header_position")?
            .and_then(|value| TabHeaderPosition::parse(&value))
            .unwrap_or(TabHeaderPosition::Top);

        let mut tabs = TabView::new()
            .selection(Binding::new(selection))
            .header_position(header_position);
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            tabs = tabs.on_change_callback(cb);
        }

        for (idx, child) in spec.children.iter().enumerate() {
            let title = child
                .meta
                .get("title")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Tab{}", idx + 1));
            let view = registry.build(&child.node)?;
            tabs.add_tab(title, view);
        }

        Ok(wrap_with_id(spec, Box::new(tabs)))
    });
}

fn register_stack<T: StackBuilder + Component + ComponentPropertySchema + 'static>(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    name: &str,
    axis: StackAxis,
) {
    let schema = component_schema::<T>(name).allow_children(true);

    registry.register(schema, move |spec, registry| {
        let spacing = prop_u16(spec, "spacing")?.unwrap_or(0);
        let padding = prop_edge_insets(spec, "padding")?.unwrap_or(EdgeInsets::ZERO);
        let scrollable = prop_bool(spec, "scrollable")?.unwrap_or(false);
        let mut stack = match axis {
            StackAxis::Vertical => T::new().with_spacing(spacing).with_padding(padding),
            StackAxis::Horizontal => T::new().with_spacing(spacing).with_padding(padding),
        };
        if scrollable {
            stack = stack.with_scrollable(scrollable);
        }

        for child in &spec.children {
            let view = registry.build(&child.node)?;
            let layout = child
                .layout
                .as_ref()
                .map(layout_from_spec)
                .unwrap_or_default();
            stack.add_child_with_layout(view, layout);
        }

        Ok(wrap_with_id(spec, Box::new(stack)))
    });
}

fn register_grid(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Grid>("Grid").allow_children(true);

    registry.register(schema, move |spec, registry| {
        let columns = prop_usize(spec, "columns")?.unwrap_or(1);
        let row_gap = prop_u16(spec, "row_gap")?.unwrap_or(0);
        let column_gap = prop_u16(spec, "column_gap")?.unwrap_or(0);
        let padding = prop_edge_insets(spec, "padding")?.unwrap_or(EdgeInsets::ZERO);
        let scrollable = prop_bool(spec, "scrollable")?.unwrap_or(false);

        let mut grid = Grid::new()
            .with_columns(columns)
            .with_row_gap(row_gap)
            .with_column_gap(column_gap)
            .with_padding(padding)
            .with_scrollable(scrollable);

        for child in &spec.children {
            let view = registry.build(&child.node)?;
            let layout = child
                .layout
                .as_ref()
                .map(layout_from_spec)
                .unwrap_or_default();
            grid.add_child_with_layout(view, layout);
        }

        Ok(wrap_with_id(spec, Box::new(grid)))
    });
}

fn register_splitter(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Splitter>("Splitter").allow_children(true);

    registry.register(schema, move |spec, registry| {
        let orientation = prop_string(spec, "orientation")?
            .and_then(|value| SplitterOrientation::parse(&value))
            .unwrap_or(SplitterOrientation::Vertical);

        let first = spec
            .children
            .first()
            .map(|child| registry.build(&child.node))
            .transpose()?
            .unwrap_or_else(|| Box::new(Spacer::new()));
        let second = spec
            .children
            .get(1)
            .map(|child| registry.build(&child.node))
            .transpose()?
            .unwrap_or_else(|| Box::new(Spacer::new()));

        let mut splitter = Splitter::new(orientation, first, second);

        if let Some(split_pos) = prop_u16(spec, "split_pos")? {
            splitter.set_split_position(split_pos);
        }
        if let Some(min_first) = prop_u16(spec, "min_first")? {
            splitter = splitter.min_first(min_first);
        }
        if let Some(min_second) = prop_u16(spec, "min_second")? {
            splitter = splitter.min_second(min_second);
        }
        let border = prop_bool(spec, "border")?.unwrap_or(true);
        splitter = splitter.with_border(border);

        Ok(wrap_with_id(spec, Box::new(splitter)))
    });
}

fn register_divider(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Divider>("Divider").allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let orientation = prop_string(spec, "orientation")?
            .and_then(|value| DividerOrientation::parse(&value))
            .unwrap_or(DividerOrientation::Horizontal);
        let view = Divider::new(orientation);
        Ok(wrap_with_id(spec, Box::new(view)))
    });
}

fn register_spacer(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Spacer>("Spacer").allow_children(false);

    registry.register(schema, move |spec, _registry| {
        Ok(wrap_with_id(spec, Box::new(Spacer::new())))
    });
}

fn register_border(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Border>("Border").allow_children(true);

    registry.register(schema, move |spec, registry| {
        let inner = spec
            .children
            .first()
            .map(|child| registry.build(&child.node))
            .transpose()?
            .unwrap_or_else(|| Box::new(Spacer::new()));
        let border = prop_bool(spec, "border")?.unwrap_or(true);
        let view = Border::new(inner).with_border(border);
        Ok(wrap_with_id(spec, Box::new(view)))
    });
}

fn register_visibility(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Visibility>("Visibility").allow_children(true);

    registry.register(schema, move |spec, registry| {
        let visible = prop_bool(spec, "visible")?.unwrap_or(true);
        let inner = spec
            .children
            .first()
            .map(|child| registry.build(&child.node))
            .transpose()?
            .unwrap_or_else(|| Box::new(Spacer::new()));
        let view = Visibility::new(Binding::new(visible), inner);
        Ok(wrap_with_id(spec, Box::new(view)))
    });
}

pub fn wrap_with_id(spec: &ComponentSpec, view: Box<dyn Component>) -> Box<dyn Component> {
    match &spec.id {
        Some(id) => Box::new(ComponentTag::boxed(id.clone(), view)),
        None => view,
    }
}

pub fn event_handle(
    spec: &ComponentSpec,
    name: &str,
    callbacks: CallbackRegistry,
) -> Option<CallbackHandle> {
    let callback: CallbackId = spec.events.get(name).copied()?;
    Some(CallbackHandle::new(
        callbacks,
        callback,
        spec.id.clone(),
        name.to_string(),
    ))
}

trait StackBuilder {
    fn new() -> Self;
    fn with_spacing(self, spacing: impl Into<Binding<u16>>) -> Self;
    fn with_padding(self, padding: impl Into<Binding<EdgeInsets>>) -> Self;
    fn with_scrollable(self, scrollable: impl Into<Binding<bool>>) -> Self;
    fn add_child_with_layout(&mut self, view: Box<dyn Component>, layout: LayoutParams);
}

impl StackBuilder for VStack {
    fn new() -> Self {
        VStack::new()
    }

    fn with_spacing(self, spacing: impl Into<Binding<u16>>) -> Self {
        self.with_spacing(spacing)
    }

    fn with_padding(self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
        self.with_padding(padding)
    }

    fn with_scrollable(self, scrollable: impl Into<Binding<bool>>) -> Self {
        self.with_scrollable(scrollable)
    }

    fn add_child_with_layout(&mut self, view: Box<dyn Component>, layout: LayoutParams) {
        self.add_child_with_layout(view, layout);
    }
}

impl StackBuilder for HStack {
    fn new() -> Self {
        HStack::new()
    }

    fn with_spacing(self, spacing: impl Into<Binding<u16>>) -> Self {
        self.with_spacing(spacing)
    }

    fn with_padding(self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
        self.with_padding(padding)
    }

    fn with_scrollable(self, scrollable: impl Into<Binding<bool>>) -> Self {
        self.with_scrollable(scrollable)
    }

    fn add_child_with_layout(&mut self, view: Box<dyn Component>, layout: LayoutParams) {
        self.add_child_with_layout(view, layout);
    }
}
