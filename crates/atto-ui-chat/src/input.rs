use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use atto_ui::composable::{
    Component, ComponentContext, Divider, EventResult, HStack, LayoutParams, Size, Spacer, Text,
    VStack,
};
use atto_ui::reactive::{Binding, DirtyObserver, Property};
use atto_ui::widgets::{Button, RadioGroup, TextArea, TextBox};
use atto_ui::{ComponentError, ComponentValue, ComponentValueCodec};
use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatTextInputConfig {
    pub title: String,
    pub placeholder: Option<String>,
    pub height: u16,
}

impl ChatTextInputConfig {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            placeholder: None,
            height: 5,
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn height(mut self, height: u16) -> Self {
        self.height = height.max(3);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatChoiceInputConfig {
    pub prompt: String,
    pub options: Vec<String>,
    pub allow_custom: bool,
    pub submit_label: String,
}

impl ChatChoiceInputConfig {
    pub fn new(prompt: impl Into<String>, options: Vec<String>) -> Self {
        Self {
            prompt: prompt.into(),
            options,
            allow_custom: false,
            submit_label: "Submit".to_string(),
        }
    }

    pub fn allow_custom(mut self, allow: bool) -> Self {
        self.allow_custom = allow;
        self
    }

    pub fn submit_label(mut self, label: impl Into<String>) -> Self {
        self.submit_label = label.into();
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatConfirmInputConfig {
    pub prompt: String,
    pub yes_label: String,
    pub no_label: String,
    pub allow_custom: bool,
}

impl ChatConfirmInputConfig {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            yes_label: "Yes".to_string(),
            no_label: "No".to_string(),
            allow_custom: false,
        }
    }

    pub fn yes_label(mut self, label: impl Into<String>) -> Self {
        self.yes_label = label.into();
        self
    }

    pub fn no_label(mut self, label: impl Into<String>) -> Self {
        self.no_label = label.into();
        self
    }

    pub fn allow_custom(mut self, allow: bool) -> Self {
        self.allow_custom = allow;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatInputMode {
    Text(ChatTextInputConfig),
    Choice(ChatChoiceInputConfig),
    Confirm(ChatConfirmInputConfig),
    Custom,
}

impl ChatInputMode {
    pub fn text(title: impl Into<String>, placeholder: impl Into<Option<String>>) -> Self {
        let mut cfg = ChatTextInputConfig::new(title);
        if let Some(ph) = placeholder.into() {
            cfg.placeholder = Some(ph);
        }
        ChatInputMode::Text(cfg)
    }

    pub fn choice(prompt: impl Into<String>, options: Vec<String>) -> Self {
        ChatInputMode::Choice(ChatChoiceInputConfig::new(prompt, options))
    }

    pub fn confirm(prompt: impl Into<String>) -> Self {
        ChatInputMode::Confirm(ChatConfirmInputConfig::new(prompt))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatInputResponse {
    Text(String),
    Choice { index: usize, label: String },
    Custom(String),
}

fn normalize_mode_kind(kind: &str) -> String {
    kind.chars()
        .filter(|c| !matches!(c, '_' | '-' | ' '))
        .flat_map(|c| c.to_lowercase())
        .collect()
}

pub(crate) fn chat_input_mode_to_component_value(mode: &ChatInputMode) -> ComponentValue {
    let mut map = BTreeMap::<String, ComponentValue>::new();
    match mode {
        ChatInputMode::Text(cfg) => {
            map.insert(
                "type".to_string(),
                ComponentValue::String("text".to_string()),
            );
            map.insert(
                "title".to_string(),
                ComponentValue::String(cfg.title.clone()),
            );
            map.insert(
                "placeholder".to_string(),
                cfg.placeholder
                    .as_ref()
                    .map(|ph| ComponentValue::String(ph.clone()))
                    .unwrap_or(ComponentValue::Null),
            );
            map.insert("height".to_string(), ComponentValue::U64(cfg.height as u64));
        }
        ChatInputMode::Choice(cfg) => {
            map.insert(
                "type".to_string(),
                ComponentValue::String("choice".to_string()),
            );
            map.insert(
                "prompt".to_string(),
                ComponentValue::String(cfg.prompt.clone()),
            );
            map.insert(
                "options".to_string(),
                ComponentValue::StringList(cfg.options.clone()),
            );
            map.insert(
                "allow_custom".to_string(),
                ComponentValue::Bool(cfg.allow_custom),
            );
            map.insert(
                "submit_label".to_string(),
                ComponentValue::String(cfg.submit_label.clone()),
            );
        }
        ChatInputMode::Confirm(cfg) => {
            map.insert(
                "type".to_string(),
                ComponentValue::String("confirm".to_string()),
            );
            map.insert(
                "prompt".to_string(),
                ComponentValue::String(cfg.prompt.clone()),
            );
            map.insert(
                "yes_label".to_string(),
                ComponentValue::String(cfg.yes_label.clone()),
            );
            map.insert(
                "no_label".to_string(),
                ComponentValue::String(cfg.no_label.clone()),
            );
            map.insert(
                "allow_custom".to_string(),
                ComponentValue::Bool(cfg.allow_custom),
            );
        }
        ChatInputMode::Custom => {
            map.insert(
                "type".to_string(),
                ComponentValue::String("custom".to_string()),
            );
        }
    }
    ComponentValue::Map(map)
}

fn parse_string_list_value(value: &ComponentValue) -> Result<Vec<String>, String> {
    match value {
        ComponentValue::StringList(values) => Ok(values.clone()),
        ComponentValue::List(values) => {
            let mut out = Vec::with_capacity(values.len());
            for cell in values {
                let Some(s) = cell.as_str() else {
                    return Err(format!("options list must contain strings, got {cell:?}"));
                };
                out.push(s.to_string());
            }
            Ok(out)
        }
        other => Err(format!("expected string list, got {other:?}")),
    }
}

fn parse_optional_string_value(
    value: &ComponentValue,
    field: &str,
) -> Result<Option<String>, String> {
    match value {
        ComponentValue::Null => Ok(None),
        ComponentValue::String(v) => Ok(Some(v.clone())),
        other => Err(format!(
            "expected string or null for '{field}', got {other:?}"
        )),
    }
}

fn parse_string_value(value: &ComponentValue, field: &str) -> Result<String, String> {
    value
        .as_str()
        .map(|v| v.to_string())
        .ok_or_else(|| format!("expected string for '{field}', got {value:?}"))
}

fn parse_bool_value(value: &ComponentValue, field: &str) -> Result<bool, String> {
    match value {
        ComponentValue::Bool(v) => Ok(*v),
        other => Err(format!("expected bool for '{field}', got {other:?}")),
    }
}

fn parse_u16_value(value: &ComponentValue, field: &str) -> Result<u16, String> {
    let raw = value
        .as_u64()
        .ok_or_else(|| format!("expected unsigned integer for '{field}', got {value:?}"))?;
    u16::try_from(raw).map_err(|_| format!("'{field}' value {raw} exceeds u16 range"))
}

pub(crate) fn parse_chat_input_mode_value(value: &ComponentValue) -> Result<ChatInputMode, String> {
    match value {
        ComponentValue::Null => Ok(ChatInputMode::text(
            "Message",
            Some("Type a message...".to_string()),
        )),
        ComponentValue::String(raw) => match normalize_mode_kind(raw).as_str() {
            "text" => Ok(ChatInputMode::text(
                "Message",
                Some("Type a message...".to_string()),
            )),
            "choice" => Ok(ChatInputMode::choice("Choose", Vec::new())),
            "confirm" => Ok(ChatInputMode::confirm("Confirm")),
            "custom" => Ok(ChatInputMode::Custom),
            _ => Err(format!("unknown mode '{raw}'")),
        },
        ComponentValue::Map(map) => {
            let kind_raw = map
                .get("type")
                .and_then(ComponentValue::as_str)
                .ok_or_else(|| "mode map must contain string field 'type'".to_string())?;
            match normalize_mode_kind(kind_raw).as_str() {
                "text" => {
                    let title = map
                        .get("title")
                        .map(|v| parse_string_value(v, "title"))
                        .transpose()?
                        .unwrap_or_else(|| "Message".to_string());
                    let placeholder = if let Some(v) = map.get("placeholder") {
                        parse_optional_string_value(v, "placeholder")?
                    } else {
                        Some("Type a message...".to_string())
                    };
                    Ok(ChatInputMode::Text(ChatTextInputConfig {
                        title,
                        placeholder,
                        height: map
                            .get("height")
                            .map(|v| parse_u16_value(v, "height"))
                            .transpose()?
                            .unwrap_or(5)
                            .max(3),
                    }))
                }
                "choice" => {
                    let prompt = map
                        .get("prompt")
                        .map(|v| parse_string_value(v, "prompt"))
                        .transpose()?
                        .unwrap_or_else(|| "Choose".to_string());
                    let options = map
                        .get("options")
                        .map(parse_string_list_value)
                        .transpose()?
                        .unwrap_or_default();
                    let allow_custom = map
                        .get("allow_custom")
                        .map(|v| parse_bool_value(v, "allow_custom"))
                        .transpose()?
                        .unwrap_or(false);
                    let submit_label = map
                        .get("submit_label")
                        .map(|v| parse_string_value(v, "submit_label"))
                        .transpose()?
                        .unwrap_or_else(|| "Submit".to_string());
                    Ok(ChatInputMode::Choice(ChatChoiceInputConfig {
                        prompt,
                        options,
                        allow_custom,
                        submit_label,
                    }))
                }
                "confirm" => {
                    let prompt = map
                        .get("prompt")
                        .map(|v| parse_string_value(v, "prompt"))
                        .transpose()?
                        .unwrap_or_else(|| "Confirm".to_string());
                    let yes_label = map
                        .get("yes_label")
                        .map(|v| parse_string_value(v, "yes_label"))
                        .transpose()?
                        .unwrap_or_else(|| "Yes".to_string());
                    let no_label = map
                        .get("no_label")
                        .map(|v| parse_string_value(v, "no_label"))
                        .transpose()?
                        .unwrap_or_else(|| "No".to_string());
                    let allow_custom = map
                        .get("allow_custom")
                        .map(|v| parse_bool_value(v, "allow_custom"))
                        .transpose()?
                        .unwrap_or(false);
                    Ok(ChatInputMode::Confirm(ChatConfirmInputConfig {
                        prompt,
                        yes_label,
                        no_label,
                        allow_custom,
                    }))
                }
                "custom" => Ok(ChatInputMode::Custom),
                other => Err(format!("unknown mode type '{other}'")),
            }
        }
        other => Err(format!("expected string or map, got {other:?}")),
    }
}

pub(crate) fn chat_input_response_to_component_value(resp: ChatInputResponse) -> ComponentValue {
    let mut out = BTreeMap::<String, ComponentValue>::new();
    match resp {
        ChatInputResponse::Text(text) => {
            out.insert(
                "type".to_string(),
                ComponentValue::String("text".to_string()),
            );
            out.insert("text".to_string(), ComponentValue::String(text));
        }
        ChatInputResponse::Choice { index, label } => {
            out.insert(
                "type".to_string(),
                ComponentValue::String("choice".to_string()),
            );
            out.insert("index".to_string(), ComponentValue::U64(index as u64));
            out.insert("label".to_string(), ComponentValue::String(label));
        }
        ChatInputResponse::Custom(text) => {
            out.insert(
                "type".to_string(),
                ComponentValue::String("custom".to_string()),
            );
            out.insert("text".to_string(), ComponentValue::String(text));
        }
    }
    ComponentValue::Map(out)
}

#[derive(Clone, Debug)]
pub struct ChatInputHandle {
    mode: Property<ChatInputMode>,
    draft: Property<String>,
    custom: Property<String>,
    history: Property<Vec<String>>,
    selection: Property<usize>,
    enabled: Property<bool>,
    clear_on_submit: Property<bool>,
}

impl ChatInputHandle {
    pub fn new() -> Self {
        Self {
            mode: Property::new(ChatInputMode::text(
                "Message",
                Some("Type a message...".into()),
            )),
            draft: Property::new(String::new()),
            custom: Property::new(String::new()),
            history: Property::new(Vec::new()),
            selection: Property::new(0),
            enabled: Property::new(true),
            clear_on_submit: Property::new(true),
        }
    }

    pub fn panel(&self) -> ChatInputPanel {
        ChatInputPanel::from_handle(self)
    }

    pub fn mode(&self) -> ChatInputMode {
        self.mode.get()
    }

    pub fn set_mode(&self, mode: ChatInputMode) {
        self.mode.set(mode);
    }

    pub fn draft_binding(&self) -> Binding<String> {
        self.draft.binding()
    }

    pub fn custom_binding(&self) -> Binding<String> {
        self.custom.binding()
    }

    pub fn history_binding(&self) -> Binding<Vec<String>> {
        self.history.binding()
    }

    pub fn selection_binding(&self) -> Binding<usize> {
        self.selection.binding()
    }

    pub fn enabled_binding(&self) -> Binding<bool> {
        self.enabled.binding()
    }

    pub fn clear_on_submit_binding(&self) -> Binding<bool> {
        self.clear_on_submit.binding()
    }
}

impl Default for ChatInputHandle {
    fn default() -> Self {
        Self::new()
    }
}

enum ChatInputView {
    Text(Box<TextArea>),
    Choice(VStack),
    Confirm(VStack),
    Custom(SharedComponent),
}

pub struct ChatInputPanel {
    mode: Binding<ChatInputMode>,
    draft: Binding<String>,
    custom: Binding<String>,
    history: Binding<Vec<String>>,
    selection: Binding<usize>,
    enabled: Binding<bool>,
    clear_on_submit: Binding<bool>,
    view: ChatInputView,
    mode_observer: DirtyObserver,
    on_submit: Option<Arc<dyn Fn(ChatInputResponse) + Send + Sync>>,
    custom_view: Option<Arc<Mutex<Box<dyn Component>>>>,
}

impl ChatInputPanel {
    pub fn from_handle(handle: &ChatInputHandle) -> Self {
        let mode = handle.mode.binding();
        let draft = handle.draft.binding();
        let custom = handle.custom.binding();
        let history = handle.history.binding();
        let selection = handle.selection.binding();
        let enabled = handle.enabled.binding();
        let clear_on_submit = handle.clear_on_submit.binding();
        let mut panel = Self {
            mode: mode.clone(),
            draft: draft.clone(),
            custom: custom.clone(),
            history: history.clone(),
            selection: selection.clone(),
            enabled: enabled.clone(),
            clear_on_submit: clear_on_submit.clone(),
            view: ChatInputView::Text(Box::new(
                TextArea::new("", draft.clone()).history(history.clone()),
            )),
            mode_observer: mode.dirty_observer(),
            on_submit: None,
            custom_view: None,
        };
        panel.view = panel.build_view(&mode.get());
        panel
    }

    pub fn on_submit<F>(mut self, callback: F) -> Self
    where
        F: Fn(ChatInputResponse) + Send + Sync + 'static,
    {
        self.on_submit = Some(Arc::new(callback));
        self
    }

    pub fn set_custom_view(&mut self, view: impl Component + 'static) {
        self.custom_view = Some(Arc::new(Mutex::new(Box::new(view))));
        self.view = self.build_view(&self.mode.get());
    }

    fn sync_mode(&mut self) {
        if self.mode.check_dirty(&mut self.mode_observer) {
            self.view = self.build_view(&self.mode.get());
        }
    }

    fn build_view(&self, mode: &ChatInputMode) -> ChatInputView {
        let content_layout = LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        };
        match mode {
            ChatInputMode::Text(cfg) => {
                let mut input = TextArea::new(cfg.title.clone(), self.draft.clone())
                    .enabled(self.enabled.clone())
                    .history(self.history.clone())
                    .height(cfg.height)
                    .enter_submits(true);
                if let Some(ph) = &cfg.placeholder {
                    input = input.placeholder(ph.clone());
                }
                ChatInputView::Text(Box::new(input))
            }
            ChatInputMode::Choice(cfg) => {
                if !cfg.options.is_empty() {
                    let idx = self
                        .selection
                        .get()
                        .min(cfg.options.len().saturating_sub(1));
                    self.selection.set(idx);
                }
                let options_binding: Binding<Vec<String>> = cfg.options.clone().into();
                let radio =
                    RadioGroup::new(cfg.prompt.clone(), options_binding, self.selection.clone())
                        .enabled(self.enabled.clone());
                let mut column = VStack::new().with_spacing(1);
                column = column.child_with_layout(radio, content_layout);
                if cfg.allow_custom {
                    let custom = TextBox::new("Custom", self.custom.clone())
                        .placeholder("Type a custom reply")
                        .enabled(self.enabled.clone());
                    column = column.child_with_layout(custom, content_layout);
                }
                let submit_label = cfg.submit_label.clone();
                let submit_width = button_width(&submit_label);
                let submit = Button::new(submit_label).enabled(self.enabled.clone());
                let submit_layout = LayoutParams {
                    width: Size::Fixed(submit_width),
                    height: Size::Content,
                    ..LayoutParams::default()
                };
                let buttons = HStack::new()
                    .with_spacing(1)
                    .child_with_layout(submit, submit_layout)
                    .child(Spacer::new());
                column = column
                    .child_with_layout(Divider::horizontal(), content_layout)
                    .child_with_layout(buttons, content_layout);
                ChatInputView::Choice(column)
            }
            ChatInputMode::Confirm(cfg) => {
                let idx = self.selection.get().min(1);
                self.selection.set(idx);
                let selection_yes = self.selection.clone();
                let custom_clear_yes = self.custom.clone();
                let yes_label = cfg.yes_label.clone();
                let yes_button = Button::new(yes_label).on_click(move || {
                    selection_yes.set(0);
                    custom_clear_yes.set(String::new());
                });

                let selection_no = self.selection.clone();
                let custom_clear_no = self.custom.clone();
                let no_label = cfg.no_label.clone();
                let no_button = Button::new(no_label).on_click(move || {
                    selection_no.set(1);
                    custom_clear_no.set(String::new());
                });

                let yes_width = button_width(&cfg.yes_label);
                let no_width = button_width(&cfg.no_label);
                let buttons = HStack::new()
                    .with_spacing(1)
                    .child_with_layout(
                        yes_button.enabled(self.enabled.clone()),
                        LayoutParams {
                            width: Size::Fixed(yes_width),
                            height: Size::Content,
                            ..LayoutParams::default()
                        },
                    )
                    .child_with_layout(
                        no_button.enabled(self.enabled.clone()),
                        LayoutParams {
                            width: Size::Fixed(no_width),
                            height: Size::Content,
                            ..LayoutParams::default()
                        },
                    )
                    .child(Spacer::new());

                let mut column = VStack::new().with_spacing(1);
                column = column.child_with_layout(Text::new(cfg.prompt.clone()), content_layout);
                column = column.child_with_layout(buttons, content_layout);
                if cfg.allow_custom {
                    let custom = TextBox::new("Custom", self.custom.clone())
                        .placeholder("Type a custom reply")
                        .enabled(self.enabled.clone());
                    column = column.child_with_layout(custom, content_layout);
                }
                ChatInputView::Confirm(column)
            }
            ChatInputMode::Custom => {
                if let Some(view) = &self.custom_view {
                    ChatInputView::Custom(SharedComponent::new(view.clone()))
                } else {
                    let fallback = TextArea::new("Message", self.draft.clone())
                        .placeholder("Type a message")
                        .enabled(self.enabled.clone())
                        .history(self.history.clone())
                        .enter_submits(true);
                    ChatInputView::Text(Box::new(fallback))
                }
            }
        }
    }

    fn emit_response(&mut self) -> bool {
        let Some(cb) = &self.on_submit else {
            return false;
        };

        match self.mode.get() {
            ChatInputMode::Text(_) => {
                let text = self.draft.get();
                if text.trim().is_empty() {
                    return false;
                }
                cb(ChatInputResponse::Text(text.clone()));
                if self.clear_on_submit.get() {
                    self.draft.set(String::new());
                }
                true
            }
            ChatInputMode::Choice(cfg) => {
                let custom = self.custom.get();
                if cfg.allow_custom && !custom.trim().is_empty() {
                    cb(ChatInputResponse::Custom(custom.clone()));
                    if self.clear_on_submit.get() {
                        self.custom.set(String::new());
                    }
                    return true;
                }
                if cfg.options.is_empty() {
                    return false;
                }
                let idx = self
                    .selection
                    .get()
                    .min(cfg.options.len().saturating_sub(1));
                let label = cfg.options.get(idx).cloned().unwrap_or_default();
                cb(ChatInputResponse::Choice { index: idx, label });
                if self.clear_on_submit.get() {
                    self.custom.set(String::new());
                }
                true
            }
            ChatInputMode::Confirm(cfg) => {
                let custom = self.custom.get();
                if cfg.allow_custom && !custom.trim().is_empty() {
                    cb(ChatInputResponse::Custom(custom.clone()));
                    if self.clear_on_submit.get() {
                        self.custom.set(String::new());
                    }
                    return true;
                }
                let labels = [cfg.yes_label, cfg.no_label];
                let idx = self.selection.get().min(labels.len().saturating_sub(1));
                let label = labels.get(idx).cloned().unwrap_or_default();
                cb(ChatInputResponse::Choice { index: idx, label });
                if self.clear_on_submit.get() {
                    self.custom.set(String::new());
                }
                true
            }
            ChatInputMode::Custom => false,
        }
    }

    fn estimated_height_for_mode(&self, mode: &ChatInputMode) -> u16 {
        const TEXTBOX_HEIGHT: u16 = 3;
        const BUTTON_HEIGHT: u16 = 3;
        const DIVIDER_HEIGHT: u16 = 1;
        const SPACING: u16 = 1;

        match mode {
            ChatInputMode::Text(cfg) => cfg.height.max(3),
            ChatInputMode::Choice(cfg) => {
                let radio_height = cfg.options.len().saturating_add(1) as u16;
                let mut parts: u16 = 3; // radio + divider + buttons
                let mut total = radio_height + DIVIDER_HEIGHT + BUTTON_HEIGHT;
                if cfg.allow_custom {
                    parts += 1;
                    total = total.saturating_add(TEXTBOX_HEIGHT);
                }
                total.saturating_add(SPACING.saturating_mul(parts.saturating_sub(1)))
            }
            ChatInputMode::Confirm(cfg) => {
                let mut parts: u16 = 2; // prompt + buttons
                let mut total = 1 + BUTTON_HEIGHT;
                if cfg.allow_custom {
                    parts += 1;
                    total = total.saturating_add(TEXTBOX_HEIGHT);
                }
                total.saturating_add(SPACING.saturating_mul(parts.saturating_sub(1)))
            }
            ChatInputMode::Custom => {
                if let Some(view) = &self.custom_view {
                    let guard = view.lock().unwrap();
                    guard.desired_height().unwrap_or(TEXTBOX_HEIGHT)
                } else {
                    TEXTBOX_HEIGHT
                }
            }
        }
    }
}

impl ::atto_ui::composable::Component for ChatInputPanel {
    fn property_names(&self) -> Vec<&'static str> {
        vec![
            "mode",
            "draft",
            "custom",
            "history",
            "selection",
            "enabled",
            "clear_on_submit",
        ]
    }

    fn get_property(&self, name: &str) -> Option<ComponentValue> {
        match name {
            "mode" => Some(chat_input_mode_to_component_value(&self.mode.get())),
            "draft" => Some(ComponentValue::String(self.draft.get())),
            "custom" => Some(ComponentValue::String(self.custom.get())),
            "history" => Some(ComponentValue::StringList(self.history.get())),
            "selection" => Some(ComponentValue::U64(self.selection.get() as u64)),
            "enabled" => Some(ComponentValue::Bool(self.enabled.get())),
            "clear_on_submit" => Some(ComponentValue::Bool(self.clear_on_submit.get())),
            _ => None,
        }
    }

    fn set_property(&mut self, name: &str, value: ComponentValue) -> Result<(), ComponentError> {
        match name {
            "mode" => {
                let mode = parse_chat_input_mode_value(&value)
                    .map_err(|_| ComponentError::invalid_value(name, "chat input mode"))?;
                self.mode.set(mode);
                self.sync_mode();
                Ok(())
            }
            "draft" => {
                let draft = <String as ComponentValueCodec>::from_component_value(value, name)?;
                self.draft.set(draft);
                Ok(())
            }
            "custom" => {
                let custom = <String as ComponentValueCodec>::from_component_value(value, name)?;
                self.custom.set(custom);
                Ok(())
            }
            "history" => {
                let history =
                    <Vec<String> as ComponentValueCodec>::from_component_value(value, name)?;
                self.history.set(history);
                Ok(())
            }
            "selection" => {
                let selection = <usize as ComponentValueCodec>::from_component_value(value, name)?;
                let selection = match &self.mode.get() {
                    ChatInputMode::Choice(cfg) if !cfg.options.is_empty() => {
                        selection.min(cfg.options.len().saturating_sub(1))
                    }
                    ChatInputMode::Confirm(_) => selection.min(1),
                    _ => selection,
                };
                self.selection.set(selection);
                Ok(())
            }
            "enabled" => {
                let enabled = <bool as ComponentValueCodec>::from_component_value(value, name)?;
                self.enabled.set(enabled);
                Ok(())
            }
            "clear_on_submit" => {
                let clear = <bool as ComponentValueCodec>::from_component_value(value, name)?;
                self.clear_on_submit.set(clear);
                Ok(())
            }
            _ => Err(ComponentError::unsupported_property(name)),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.sync_mode();
        match &mut self.view {
            ChatInputView::Text(view) => view.draw(frame, area, ctx),
            ChatInputView::Choice(view) => view.draw(frame, area, ctx),
            ChatInputView::Confirm(view) => view.draw(frame, area, ctx),
            ChatInputView::Custom(view) => view.draw(frame, area, ctx),
        }
    }
}

impl ::atto_ui::composable::DragAndDrop for ChatInputPanel {}

impl ::atto_ui::composable::Layout for ChatInputPanel {
    fn min_width(&self) -> u16 {
        match &self.view {
            ChatInputView::Text(view) => view.min_width(),
            ChatInputView::Choice(view) => view.min_width(),
            ChatInputView::Confirm(view) => view.min_width(),
            ChatInputView::Custom(view) => view.min_width(),
        }
    }

    fn min_height(&self) -> u16 {
        match self.mode.get() {
            ChatInputMode::Text(_) => 3,
            _ => 3,
        }
    }

    fn desired_width(&self) -> Option<u16> {
        match &self.view {
            ChatInputView::Text(view) => view.desired_width(),
            ChatInputView::Choice(view) => view.desired_width(),
            ChatInputView::Confirm(view) => view.desired_width(),
            ChatInputView::Custom(view) => view.desired_width(),
        }
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.estimated_height_for_mode(&self.mode.get()))
    }
}

impl ::atto_ui::composable::Scrollable for ChatInputPanel {}

impl ::atto_ui::composable::FocusNav for ChatInputPanel {
    fn is_focusable(&self) -> bool {
        match &self.view {
            ChatInputView::Text(view) => view.is_focusable(),
            ChatInputView::Choice(view) => view.is_focusable(),
            ChatInputView::Confirm(view) => view.is_focusable(),
            ChatInputView::Custom(view) => view.is_focusable(),
        }
    }

    fn focus_first(&mut self) -> bool {
        match &mut self.view {
            ChatInputView::Text(view) => view.focus_first(),
            ChatInputView::Choice(view) => view.focus_first(),
            ChatInputView::Confirm(view) => view.focus_first(),
            ChatInputView::Custom(view) => view.focus_first(),
        }
    }

    fn focus_last(&mut self) -> bool {
        match &mut self.view {
            ChatInputView::Text(view) => view.focus_last(),
            ChatInputView::Choice(view) => view.focus_last(),
            ChatInputView::Confirm(view) => view.focus_last(),
            ChatInputView::Custom(view) => view.focus_last(),
        }
    }
}

impl ::atto_ui::composable::DynamicTree for ChatInputPanel {}

impl ::atto_ui::composable::EventHandling for ChatInputPanel {
    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.sync_mode();
        match &mut self.view {
            ChatInputView::Text(view) => view.handle_event_capture(event, ctx),
            ChatInputView::Choice(view) => view.handle_event_capture(event, ctx),
            ChatInputView::Confirm(view) => view.handle_event_capture(event, ctx),
            ChatInputView::Custom(view) => view.handle_event_capture(event, ctx),
        }
    }

    fn handle_event_bubble(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.sync_mode();
        match &mut self.view {
            ChatInputView::Text(view) => view.handle_event_bubble(event, ctx),
            ChatInputView::Choice(view) => view.handle_event_bubble(event, ctx),
            ChatInputView::Confirm(view) => view.handle_event_bubble(event, ctx),
            ChatInputView::Custom(view) => view.handle_event_bubble(event, ctx),
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.sync_mode();
        let res = match &mut self.view {
            ChatInputView::Text(view) => view.handle_event(event, ctx),
            ChatInputView::Choice(view) => view.handle_event(event, ctx),
            ChatInputView::Confirm(view) => view.handle_event(event, ctx),
            ChatInputView::Custom(view) => view.handle_event(event, ctx),
        };

        if matches!(res.action, atto_ui::composable::ComponentAction::Submitted) {
            let _ = self.emit_response();
        }

        res
    }
}

#[derive(Clone)]
struct SharedComponent {
    inner: Arc<Mutex<Box<dyn Component>>>,
}

impl SharedComponent {
    fn new(inner: Arc<Mutex<Box<dyn Component>>>) -> Self {
        Self { inner }
    }
}

impl ::atto_ui::composable::Component for SharedComponent {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.inner.lock().unwrap().draw(frame, area, ctx)
    }
}

impl ::atto_ui::composable::DragAndDrop for SharedComponent {}

impl ::atto_ui::composable::Layout for SharedComponent {
    fn min_width(&self) -> u16 {
        self.inner.lock().unwrap().min_width()
    }

    fn min_height(&self) -> u16 {
        self.inner.lock().unwrap().min_height()
    }

    fn desired_width(&self) -> Option<u16> {
        self.inner.lock().unwrap().desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.inner.lock().unwrap().desired_height()
    }
}

impl ::atto_ui::composable::Scrollable for SharedComponent {}

impl ::atto_ui::composable::FocusNav for SharedComponent {
    fn is_focusable(&self) -> bool {
        self.inner.lock().unwrap().is_focusable()
    }

    fn focus_first(&mut self) -> bool {
        self.inner.lock().unwrap().focus_first()
    }

    fn focus_last(&mut self) -> bool {
        self.inner.lock().unwrap().focus_last()
    }
}

impl ::atto_ui::composable::DynamicTree for SharedComponent {}

impl ::atto_ui::composable::EventHandling for SharedComponent {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.inner.lock().unwrap().handle_event(event, ctx)
    }
}

fn button_width(label: &str) -> u16 {
    let text_w = label.width().min(u16::MAX as usize) as u16;
    text_w.saturating_add(4).max(3)
}
