use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use atto_ui::composable::{
    Component, ComponentContext, Divider, EventResult, HStack, LayoutParams, Size, Spacer, Text,
    VStack,
};
use atto_ui::reactive::{Binding, DirtyObserver, Property};
use atto_ui::widgets::{Button, RadioGroup, TextArea, TextBox};
use atto_ui::{ComponentError, ComponentValue, ComponentValueCodec};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::completion::{CompletionAnchor, CompletionItem, CompletionPopup};

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatSlashCommandAction {
    #[default]
    Insert,
    Submit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatSlashCommand {
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    pub replacement: String,
    pub action: ChatSlashCommandAction,
}

impl ChatSlashCommand {
    pub fn new(label: impl Into<String>) -> Self {
        let label = normalize_slash_command_label(label.into());
        Self {
            id: default_slash_command_id(&label),
            replacement: label.clone(),
            label,
            detail: None,
            action: ChatSlashCommandAction::Insert,
        }
    }

    pub fn with_id(id: impl Into<String>, label: impl Into<String>) -> Self {
        let label = normalize_slash_command_label(label.into());
        Self {
            id: id.into(),
            replacement: label.clone(),
            label,
            detail: None,
            action: ChatSlashCommandAction::Insert,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn replacement(mut self, replacement: impl Into<String>) -> Self {
        self.replacement = replacement.into();
        self
    }

    pub fn submit_on_accept(mut self) -> Self {
        self.action = ChatSlashCommandAction::Submit;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMentionCandidate {
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    pub replacement: String,
}

impl ChatMentionCandidate {
    pub fn new(label: impl Into<String>) -> Self {
        let label = label.into();
        Self {
            id: default_mention_candidate_id(&label),
            replacement: format!("@{label}"),
            label,
            detail: None,
        }
    }

    pub fn with_id(id: impl Into<String>, label: impl Into<String>) -> Self {
        let label = label.into();
        Self {
            id: id.into(),
            replacement: format!("@{label}"),
            label,
            detail: None,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn replacement(mut self, replacement: impl Into<String>) -> Self {
        self.replacement = replacement.into();
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMentionContext {
    pub draft: String,
    pub query: String,
    pub cursor: usize,
    pub replacement_start: usize,
    pub replacement_end: usize,
}

#[derive(Clone)]
pub(crate) struct ChatTextSubmitInterceptor {
    callback: Arc<dyn Fn(String) -> bool + Send + Sync>,
}

impl ChatTextSubmitInterceptor {
    pub(crate) fn new<F>(callback: F) -> Self
    where
        F: Fn(String) -> bool + Send + Sync + 'static,
    {
        Self {
            callback: Arc::new(callback),
        }
    }

    fn submit(&self, text: String) -> bool {
        (self.callback)(text)
    }
}

impl PartialEq for ChatTextSubmitInterceptor {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.callback, &other.callback)
    }
}

impl Eq for ChatTextSubmitInterceptor {}

impl fmt::Debug for ChatTextSubmitInterceptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChatTextSubmitInterceptor(..)")
    }
}

impl ChatMentionContext {
    pub fn replacement_range(&self) -> std::ops::Range<usize> {
        self.replacement_start..self.replacement_end
    }
}

fn default_slash_commands() -> Vec<ChatSlashCommand> {
    vec![
        ChatSlashCommand::new("/help").detail("Show available commands"),
        ChatSlashCommand::new("/clear").detail("Clear the conversation"),
        ChatSlashCommand::new("/model")
            .detail("Switch model")
            .replacement("/model "),
        ChatSlashCommand::new("/review")
            .detail("Start a code review")
            .replacement("/review "),
    ]
}

fn normalize_slash_command_label(label: String) -> String {
    if label.starts_with('/') {
        label
    } else {
        format!("/{label}")
    }
}

fn default_slash_command_id(label: &str) -> String {
    label
        .trim()
        .trim_start_matches('/')
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn slash_query_from_draft(draft: &str) -> Option<String> {
    if !draft.starts_with('/') || draft.contains('\n') {
        return None;
    }
    Some(draft[1..].to_string())
}

fn slash_completion_items(commands: &[ChatSlashCommand]) -> Vec<CompletionItem> {
    commands
        .iter()
        .map(|command| {
            let mut item =
                CompletionItem::with_replacement(command.label.clone(), command.id.clone());
            if let Some(detail) = &command.detail {
                item = item.detail(detail.clone());
            }
            item
        })
        .collect()
}

fn default_mention_candidate_id(label: &str) -> String {
    label.trim().to_string()
}

fn mention_completion_items(candidates: &[ChatMentionCandidate]) -> Vec<CompletionItem> {
    candidates
        .iter()
        .map(|candidate| {
            let mut item = CompletionItem::with_replacement(
                candidate.label.clone(),
                candidate.replacement.clone(),
            );
            if let Some(detail) = &candidate.detail {
                item = item.detail(detail.clone());
            }
            item
        })
        .collect()
}

fn mention_query_from_draft_at(draft: &str, cursor: usize) -> Option<ChatMentionContext> {
    let cursor = align_to_char_boundary(draft, cursor);
    let prefix = &draft[..cursor];
    let token_start = prefix
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(idx, ch)| idx.saturating_add(ch.len_utf8()))
        .unwrap_or(0);
    let suffix = &draft[cursor..];
    let token_end = suffix
        .char_indices()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(idx, _)| cursor.saturating_add(idx))
        .unwrap_or_else(|| draft.len());
    let token = &draft[token_start..token_end];
    let query_start = token_start.saturating_add('@'.len_utf8());

    if !token.starts_with('@') || cursor < query_start {
        return None;
    }

    Some(ChatMentionContext {
        draft: draft.to_string(),
        query: draft[query_start..cursor].to_string(),
        cursor,
        replacement_start: token_start,
        replacement_end: token_end,
    })
}

fn align_to_char_boundary(text: &str, byte: usize) -> usize {
    if byte >= text.len() {
        return text.len();
    }
    let mut aligned = 0;
    for (idx, _) in text.char_indices() {
        if idx > byte {
            break;
        }
        aligned = idx;
    }
    aligned
}

fn normalize_chat_text_paste(raw: &str) -> String {
    let raw = raw.strip_prefix("\u{1b}[200~").unwrap_or(raw);
    let raw = raw.strip_suffix("\u{1b}[201~").unwrap_or(raw);
    let mut normalized = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(ch);
        }
    }

    if normalized.contains('\n') {
        trim_trailing_blank_paste_lines(&normalized)
    } else {
        normalized
    }
}

fn trim_trailing_blank_paste_lines(text: &str) -> String {
    let mut end = text.len();
    while end > 0 {
        let prefix = &text[..end];
        let line_start = prefix
            .rfind('\n')
            .map(|idx| idx + '\n'.len_utf8())
            .unwrap_or(0);
        let line = &text[line_start..end];
        if !line.chars().all(char::is_whitespace) {
            break;
        }
        if line_start == 0 {
            end = 0;
        } else {
            end = line_start - '\n'.len_utf8();
        }
    }
    text[..end].to_string()
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

pub(crate) fn chat_slash_command_to_component_value(command: &ChatSlashCommand) -> ComponentValue {
    let mut out = BTreeMap::<String, ComponentValue>::new();
    out.insert("id".to_string(), ComponentValue::String(command.id.clone()));
    out.insert(
        "label".to_string(),
        ComponentValue::String(command.label.clone()),
    );
    if let Some(detail) = &command.detail {
        out.insert("detail".to_string(), ComponentValue::String(detail.clone()));
    }
    out.insert(
        "replacement".to_string(),
        ComponentValue::String(command.replacement.clone()),
    );
    out.insert(
        "action".to_string(),
        ComponentValue::String(slash_command_action_to_string(command.action).to_string()),
    );
    ComponentValue::Map(out)
}

pub(crate) fn chat_slash_commands_to_component_value(
    commands: &[ChatSlashCommand],
) -> ComponentValue {
    ComponentValue::List(
        commands
            .iter()
            .map(chat_slash_command_to_component_value)
            .collect(),
    )
}

pub(crate) fn parse_chat_slash_commands_value(
    value: &ComponentValue,
) -> Result<Vec<ChatSlashCommand>, String> {
    let ComponentValue::List(items) = value else {
        if matches!(value, ComponentValue::Null) {
            return Ok(Vec::new());
        }
        return Err(format!("slash_commands must be list, got {value:?}"));
    };
    items.iter().map(parse_chat_slash_command_value).collect()
}

pub(crate) fn chat_mention_candidate_to_component_value(
    candidate: &ChatMentionCandidate,
) -> ComponentValue {
    let mut out = BTreeMap::<String, ComponentValue>::new();
    out.insert(
        "id".to_string(),
        ComponentValue::String(candidate.id.clone()),
    );
    out.insert(
        "label".to_string(),
        ComponentValue::String(candidate.label.clone()),
    );
    if let Some(detail) = &candidate.detail {
        out.insert("detail".to_string(), ComponentValue::String(detail.clone()));
    }
    out.insert(
        "replacement".to_string(),
        ComponentValue::String(candidate.replacement.clone()),
    );
    ComponentValue::Map(out)
}

pub(crate) fn chat_mention_candidates_to_component_value(
    candidates: &[ChatMentionCandidate],
) -> ComponentValue {
    ComponentValue::List(
        candidates
            .iter()
            .map(chat_mention_candidate_to_component_value)
            .collect(),
    )
}

pub(crate) fn parse_chat_mention_candidates_value(
    value: &ComponentValue,
) -> Result<Vec<ChatMentionCandidate>, String> {
    let ComponentValue::List(items) = value else {
        if matches!(value, ComponentValue::Null) {
            return Ok(Vec::new());
        }
        return Err(format!("mention_candidates must be list, got {value:?}"));
    };
    items
        .iter()
        .map(parse_chat_mention_candidate_value)
        .collect()
}

pub(crate) fn chat_mention_context_to_component_value(
    context: &ChatMentionContext,
) -> ComponentValue {
    let mut out = BTreeMap::<String, ComponentValue>::new();
    out.insert(
        "draft".to_string(),
        ComponentValue::String(context.draft.clone()),
    );
    out.insert(
        "query".to_string(),
        ComponentValue::String(context.query.clone()),
    );
    out.insert(
        "cursor".to_string(),
        ComponentValue::U64(context.cursor as u64),
    );
    out.insert(
        "replacement_start".to_string(),
        ComponentValue::U64(context.replacement_start as u64),
    );
    out.insert(
        "replacement_end".to_string(),
        ComponentValue::U64(context.replacement_end as u64),
    );
    ComponentValue::Map(out)
}

fn parse_chat_slash_command_value(value: &ComponentValue) -> Result<ChatSlashCommand, String> {
    let map = expect_input_map(value, "slash command")?;
    let label = required_input_string_field(map, "label", "slash command")?;
    let mut command = if let Some(id) = optional_input_string_field(map, "id", "slash command")? {
        ChatSlashCommand::with_id(id, label)
    } else {
        ChatSlashCommand::new(label)
    };
    if let Some(detail) = optional_input_string_field(map, "detail", "slash command")? {
        command = command.detail(detail);
    }
    if let Some(replacement) = optional_input_string_field(map, "replacement", "slash command")? {
        command = command.replacement(replacement);
    }
    if let Some(action) = optional_input_string_field(map, "action", "slash command")? {
        command.action = parse_slash_command_action_string(&action)?;
    }
    Ok(command)
}

fn parse_chat_mention_candidate_value(
    value: &ComponentValue,
) -> Result<ChatMentionCandidate, String> {
    let map = expect_input_map(value, "mention candidate")?;
    let label = required_input_string_field(map, "label", "mention candidate")?;
    let mut candidate =
        if let Some(id) = optional_input_string_field(map, "id", "mention candidate")? {
            ChatMentionCandidate::with_id(id, label)
        } else {
            ChatMentionCandidate::new(label)
        };
    if let Some(detail) = optional_input_string_field(map, "detail", "mention candidate")? {
        candidate = candidate.detail(detail);
    }
    if let Some(replacement) = optional_input_string_field(map, "replacement", "mention candidate")?
    {
        candidate = candidate.replacement(replacement);
    }
    Ok(candidate)
}

fn slash_command_action_to_string(action: ChatSlashCommandAction) -> &'static str {
    match action {
        ChatSlashCommandAction::Insert => "insert",
        ChatSlashCommandAction::Submit => "submit",
    }
}

fn parse_slash_command_action_string(raw: &str) -> Result<ChatSlashCommandAction, String> {
    match normalize_mode_kind(raw).as_str() {
        "insert" => Ok(ChatSlashCommandAction::Insert),
        "submit" => Ok(ChatSlashCommandAction::Submit),
        _ => Err(format!("unknown slash command action '{raw}'")),
    }
}

fn expect_input_map<'a>(
    value: &'a ComponentValue,
    context: &str,
) -> Result<&'a BTreeMap<String, ComponentValue>, String> {
    match value {
        ComponentValue::Map(map) => Ok(map),
        other => Err(format!("{context} must be map, got {other:?}")),
    }
}

fn required_input_string_field(
    map: &BTreeMap<String, ComponentValue>,
    key: &str,
    context: &str,
) -> Result<String, String> {
    match map.get(key) {
        Some(ComponentValue::String(value)) => Ok(value.clone()),
        Some(other) => Err(format!(
            "{context} field '{key}' must be string, got {other:?}"
        )),
        None => Err(format!("{context} missing {key}")),
    }
}

fn optional_input_string_field(
    map: &BTreeMap<String, ComponentValue>,
    key: &str,
    context: &str,
) -> Result<Option<String>, String> {
    match map.get(key) {
        Some(ComponentValue::String(value)) => Ok(Some(value.clone())),
        Some(ComponentValue::Null) | None => Ok(None),
        Some(other) => Err(format!(
            "{context} field '{key}' must be string, got {other:?}"
        )),
    }
}

#[derive(Clone, Debug)]
pub struct ChatInputHandle {
    mode: Property<ChatInputMode>,
    draft: Property<String>,
    custom: Property<String>,
    history: Property<Vec<String>>,
    slash_commands: Property<Vec<ChatSlashCommand>>,
    mention_candidates: Property<Vec<ChatMentionCandidate>>,
    selection: Property<usize>,
    enabled: Property<bool>,
    clear_on_submit: Property<bool>,
    streaming: Property<bool>,
    queued_responses: Property<Vec<ChatInputResponse>>,
    text_submit_interceptor: Property<Option<ChatTextSubmitInterceptor>>,
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
            slash_commands: Property::new(default_slash_commands()),
            mention_candidates: Property::new(Vec::new()),
            selection: Property::new(0),
            enabled: Property::new(true),
            clear_on_submit: Property::new(true),
            streaming: Property::new(false),
            queued_responses: Property::new(Vec::new()),
            text_submit_interceptor: Property::new(None),
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

    pub fn slash_commands(&self) -> Vec<ChatSlashCommand> {
        self.slash_commands.get()
    }

    pub fn slash_commands_binding(&self) -> Binding<Vec<ChatSlashCommand>> {
        self.slash_commands.binding()
    }

    pub fn set_slash_commands(&self, commands: Vec<ChatSlashCommand>) {
        self.slash_commands.set(commands);
    }

    pub fn register_slash_command(&self, command: ChatSlashCommand) {
        let mut commands = self.slash_commands.get();
        if let Some(existing) = commands.iter_mut().find(|item| item.id == command.id) {
            *existing = command;
        } else {
            commands.push(command);
        }
        self.slash_commands.set(commands);
    }

    pub fn mention_candidates(&self) -> Vec<ChatMentionCandidate> {
        self.mention_candidates.get()
    }

    pub fn mention_candidates_binding(&self) -> Binding<Vec<ChatMentionCandidate>> {
        self.mention_candidates.binding()
    }

    pub fn set_mention_candidates(&self, candidates: Vec<ChatMentionCandidate>) {
        self.mention_candidates.set(candidates);
    }

    pub fn register_mention_candidate(&self, candidate: ChatMentionCandidate) {
        let mut candidates = self.mention_candidates.get();
        if let Some(existing) = candidates.iter_mut().find(|item| item.id == candidate.id) {
            *existing = candidate;
        } else {
            candidates.push(candidate);
        }
        self.mention_candidates.set(candidates);
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

    /// Controls whether text submissions are queued instead of emitted.
    pub fn streaming_binding(&self) -> Binding<bool> {
        self.streaming.binding()
    }

    /// Returns the FIFO queue of text responses waiting for the next turn.
    pub fn queued_responses_binding(&self) -> Binding<Vec<ChatInputResponse>> {
        self.queued_responses.binding()
    }

    pub fn queued_responses(&self) -> Vec<ChatInputResponse> {
        self.queued_responses.get()
    }

    pub fn clear_queued_responses(&self) {
        self.queued_responses.set(Vec::new());
    }

    pub(crate) fn set_text_submit_interceptor(&self, interceptor: ChatTextSubmitInterceptor) {
        self.text_submit_interceptor.set(Some(interceptor));
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
    slash_commands: Binding<Vec<ChatSlashCommand>>,
    mention_candidates: Binding<Vec<ChatMentionCandidate>>,
    slash_query: Binding<String>,
    slash_items: Binding<Vec<CompletionItem>>,
    slash_open: Binding<bool>,
    slash_selection: Binding<usize>,
    slash_accepted: Binding<Option<CompletionItem>>,
    slash_anchor: Binding<CompletionAnchor>,
    slash_popup: CompletionPopup,
    slash_dismissed_for: Option<String>,
    mention_query: Binding<String>,
    mention_items: Binding<Vec<CompletionItem>>,
    mention_open: Binding<bool>,
    mention_selection: Binding<usize>,
    mention_accepted: Binding<Option<CompletionItem>>,
    mention_anchor: Binding<CompletionAnchor>,
    mention_popup: CompletionPopup,
    mention_active: Option<ChatMentionContext>,
    mention_dismissed_for: Option<ChatMentionContext>,
    mention_provider_key: Option<ChatMentionContext>,
    selection: Binding<usize>,
    enabled: Binding<bool>,
    clear_on_submit: Binding<bool>,
    streaming: Binding<bool>,
    queued_responses: Binding<Vec<ChatInputResponse>>,
    text_submit_interceptor: Binding<Option<ChatTextSubmitInterceptor>>,
    view: ChatInputView,
    mode_observer: DirtyObserver,
    slash_commands_observer: DirtyObserver,
    mention_candidates_observer: DirtyObserver,
    on_submit: Option<Arc<dyn Fn(ChatInputResponse) + Send + Sync>>,
    on_slash_command: Option<Arc<dyn Fn(ChatSlashCommand) + Send + Sync>>,
    on_streaming_interrupt: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    mention_provider:
        Option<Arc<dyn Fn(ChatMentionContext) -> Vec<ChatMentionCandidate> + Send + Sync>>,
    custom_view: Option<Arc<Mutex<Box<dyn Component>>>>,
}

impl ChatInputPanel {
    pub fn from_handle(handle: &ChatInputHandle) -> Self {
        let mode = handle.mode.binding();
        let draft = handle.draft.binding();
        let custom = handle.custom.binding();
        let history = handle.history.binding();
        let slash_commands = handle.slash_commands.binding();
        let mention_candidates = handle.mention_candidates.binding();
        let selection = handle.selection.binding();
        let enabled = handle.enabled.binding();
        let clear_on_submit = handle.clear_on_submit.binding();
        let streaming = handle.streaming.binding();
        let queued_responses = handle.queued_responses.binding();
        let text_submit_interceptor = handle.text_submit_interceptor.binding();
        let slash_query = Binding::new(String::new());
        let slash_items = Binding::new(slash_completion_items(&slash_commands.get()));
        let slash_open = Binding::new(false);
        let slash_selection = Binding::new(0usize);
        let slash_accepted = Binding::new(None);
        let slash_anchor = Binding::new(CompletionAnchor::default());
        let slash_popup = CompletionPopup::new(slash_query.clone(), slash_items.clone())
            .open(slash_open.clone())
            .selection(slash_selection.clone())
            .accepted(slash_accepted.clone())
            .anchor(slash_anchor.clone())
            .title("Commands")
            .empty_label("No commands");
        let mention_query = Binding::new(String::new());
        let mention_items = Binding::new(mention_completion_items(&mention_candidates.get()));
        let mention_open = Binding::new(false);
        let mention_selection = Binding::new(0usize);
        let mention_accepted = Binding::new(None);
        let mention_anchor = Binding::new(CompletionAnchor::default());
        let mention_popup = CompletionPopup::new(mention_query.clone(), mention_items.clone())
            .open(mention_open.clone())
            .selection(mention_selection.clone())
            .accepted(mention_accepted.clone())
            .anchor(mention_anchor.clone())
            .title("Files")
            .empty_label("No files");
        let mut panel = Self {
            mode: mode.clone(),
            draft: draft.clone(),
            custom: custom.clone(),
            history: history.clone(),
            slash_commands: slash_commands.clone(),
            mention_candidates: mention_candidates.clone(),
            slash_query,
            slash_items,
            slash_open,
            slash_selection,
            slash_accepted,
            slash_anchor,
            slash_popup,
            slash_dismissed_for: None,
            mention_query,
            mention_items,
            mention_open,
            mention_selection,
            mention_accepted,
            mention_anchor,
            mention_popup,
            mention_active: None,
            mention_dismissed_for: None,
            mention_provider_key: None,
            selection: selection.clone(),
            enabled: enabled.clone(),
            clear_on_submit: clear_on_submit.clone(),
            streaming,
            queued_responses,
            text_submit_interceptor,
            view: ChatInputView::Text(Box::new(
                TextArea::new("", draft.clone()).history(history.clone()),
            )),
            mode_observer: mode.dirty_observer(),
            slash_commands_observer: slash_commands.dirty_observer(),
            mention_candidates_observer: mention_candidates.dirty_observer(),
            on_submit: None,
            on_slash_command: None,
            on_streaming_interrupt: None,
            mention_provider: None,
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

    pub fn on_slash_command<F>(mut self, callback: F) -> Self
    where
        F: Fn(ChatSlashCommand) + Send + Sync + 'static,
    {
        self.on_slash_command = Some(Arc::new(callback));
        self
    }

    /// Handles an unconsumed Esc as a request to interrupt the active stream.
    pub fn on_streaming_interrupt<F>(mut self, callback: F) -> Self
    where
        F: Fn() -> bool + Send + Sync + 'static,
    {
        self.on_streaming_interrupt = Some(Arc::new(callback));
        self
    }

    pub fn mention_provider<F>(mut self, provider: F) -> Self
    where
        F: Fn(ChatMentionContext) -> Vec<ChatMentionCandidate> + Send + Sync + 'static,
    {
        self.mention_provider = Some(Arc::new(provider));
        self.mention_provider_key = None;
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

    fn sync_slash_completion(&mut self, anchor: Option<Rect>) {
        if let Some(rect) = anchor {
            self.slash_anchor.set(CompletionAnchor::new(rect));
        }

        if self
            .slash_commands
            .check_dirty(&mut self.slash_commands_observer)
        {
            self.slash_items
                .set(slash_completion_items(&self.slash_commands.get()));
        }

        let draft = self.draft.get();
        let query = if self.enabled.get() && matches!(self.mode.get(), ChatInputMode::Text(_)) {
            slash_query_from_draft(&draft)
        } else {
            None
        };

        if let Some(query) = query {
            self.slash_query.set(query);
            let dismissed = self.slash_dismissed_for.as_deref() == Some(draft.as_str());
            self.slash_open
                .set(!dismissed && !self.slash_commands.get().is_empty());
        } else {
            self.slash_query.set(String::new());
            self.slash_open.set(false);
            self.slash_selection.set(0);
            self.slash_dismissed_for = None;
        }
    }

    fn sync_mention_completion(&mut self, anchor: Option<Rect>) {
        if let Some(rect) = anchor {
            self.mention_anchor.set(CompletionAnchor::new(rect));
        }

        if self
            .mention_candidates
            .check_dirty(&mut self.mention_candidates_observer)
        {
            self.mention_items
                .set(mention_completion_items(&self.mention_candidates.get()));
        }

        let draft = self.draft.get();
        let context = if self.enabled.get() && matches!(self.mode.get(), ChatInputMode::Text(_)) {
            mention_query_from_draft_at(&draft, self.text_cursor_byte_index())
        } else {
            None
        };

        if let Some(context) = context {
            self.mention_query.set(context.query.clone());
            if let Some(provider) = &self.mention_provider
                && self.mention_provider_key.as_ref() != Some(&context)
            {
                self.mention_items
                    .set(mention_completion_items(&provider(context.clone())));
                self.mention_provider_key = Some(context.clone());
            }

            let has_source =
                self.mention_provider.is_some() || !self.mention_candidates.get().is_empty();
            let dismissed = self.mention_dismissed_for.as_ref() == Some(&context);
            self.mention_active = Some(context);
            self.mention_open.set(has_source && !dismissed);
        } else {
            self.mention_query.set(String::new());
            self.mention_open.set(false);
            self.mention_selection.set(0);
            self.mention_active = None;
            self.mention_dismissed_for = None;
            self.mention_provider_key = None;
        }
    }

    fn sync_completions(&mut self, anchor: Option<Rect>) {
        self.sync_slash_completion(anchor);
        self.sync_mention_completion(anchor);
        if self.mention_open.get() {
            self.slash_open.set(false);
        }
    }

    fn text_cursor_byte_index(&self) -> usize {
        match &self.view {
            ChatInputView::Text(view) => view.cursor_byte_index(),
            _ => self.draft.get().len(),
        }
    }

    fn handle_text_paste(&mut self, raw: &str) -> EventResult {
        if raw.is_empty() {
            return EventResult::ignored();
        }
        let text = normalize_chat_text_paste(raw);
        if text.is_empty() {
            return EventResult::consumed();
        }
        let ChatInputView::Text(view) = &mut self.view else {
            return EventResult::ignored();
        };
        let cursor = view.cursor_byte_index();
        view.replace_byte_range(cursor..cursor, &text)
    }

    fn set_draft_from_panel(&mut self, draft: String) {
        let cursor = draft.len();
        self.draft.set(draft);
        if let ChatInputView::Text(view) = &mut self.view {
            view.set_cursor_byte_index(cursor);
        }
    }

    fn dismiss_slash_completion_for_current_draft(&mut self) {
        self.slash_dismissed_for = Some(self.draft.get());
        self.slash_open.set(false);
        self.slash_selection.set(0);
    }

    fn dismiss_mention_completion_for_current_context(&mut self) {
        self.mention_dismissed_for = self.mention_active.clone();
        self.mention_open.set(false);
        self.mention_selection.set(0);
    }

    fn apply_accepted_slash_command(&mut self) -> bool {
        let Some(accepted) = self.slash_accepted.get() else {
            return false;
        };
        self.slash_accepted.set(None);

        let Some(command) = self
            .slash_commands
            .get()
            .into_iter()
            .find(|command| command.id == accepted.replacement)
        else {
            return false;
        };

        match command.action {
            ChatSlashCommandAction::Insert => {
                self.set_draft_from_panel(command.replacement.clone());
                self.slash_dismissed_for = Some(command.replacement);
            }
            ChatSlashCommandAction::Submit => {
                if let Some(callback) = &self.on_slash_command {
                    callback(command.clone());
                    if self.clear_on_submit.get() {
                        self.set_draft_from_panel(String::new());
                    }
                } else {
                    self.set_draft_from_panel(command.replacement.clone());
                    self.slash_dismissed_for = Some(command.replacement);
                }
            }
        }
        self.slash_open.set(false);
        self.slash_selection.set(0);
        true
    }

    fn apply_accepted_mention(&mut self) -> bool {
        let Some(accepted) = self.mention_accepted.get() else {
            return false;
        };
        self.mention_accepted.set(None);
        let Some(context) = self.mention_active.clone() else {
            return false;
        };

        let replacement = accepted.replacement;
        match &mut self.view {
            ChatInputView::Text(view) => {
                let _ = view.replace_byte_range(context.replacement_range(), &replacement);
            }
            _ => {
                let mut draft = self.draft.get();
                if context.replacement_start <= context.replacement_end
                    && context.replacement_end <= draft.len()
                {
                    draft.replace_range(context.replacement_range(), &replacement);
                    self.draft.set(draft);
                }
            }
        }

        let draft = self.draft.get();
        let cursor = context.replacement_start.saturating_add(replacement.len());
        self.mention_dismissed_for = mention_query_from_draft_at(&draft, cursor);
        self.mention_open.set(false);
        self.mention_selection.set(0);
        self.mention_provider_key = None;
        true
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

    fn queue_response(&self, response: ChatInputResponse) {
        self.queued_responses.update(|items| items.push(response));
    }

    fn pop_queued_response(&self) -> Option<ChatInputResponse> {
        let mut next = None;
        self.queued_responses.update_if(|items| {
            if items.is_empty() {
                false
            } else {
                next = Some(items.remove(0));
                true
            }
        });
        next
    }

    fn dispatch_response(&self, response: ChatInputResponse) -> bool {
        let Some(cb) = self.on_submit.clone() else {
            return false;
        };
        cb(response);
        true
    }

    fn emit_next_queued_response(&self) -> bool {
        if self.streaming.get() || self.on_submit.is_none() {
            return false;
        }
        let Some(response) = self.pop_queued_response() else {
            return false;
        };
        self.dispatch_response(response)
    }

    fn queue_indicator_text(&self) -> Option<String> {
        let queued = self.queued_responses.with(Vec::len);
        match (self.streaming.get(), queued) {
            (true, 0) => Some("Streaming... Enter queues new messages".to_string()),
            (true, 1) => Some("Queued 1 message while streaming".to_string()),
            (true, count) => Some(format!("Queued {count} messages while streaming")),
            (false, 1) => Some("Queued 1 message; press Enter to send next".to_string()),
            (false, count) if count > 1 => {
                Some(format!("Queued {count} messages; press Enter to send next"))
            }
            (false, _) => None,
        }
    }

    fn emit_response(&mut self) -> bool {
        match self.mode.get() {
            ChatInputMode::Text(_) => {
                let text = self.draft.get();
                if text.trim().is_empty() {
                    return self.emit_next_queued_response();
                }
                if let Some(interceptor) = self.text_submit_interceptor.get()
                    && interceptor.submit(text.clone())
                {
                    if self.clear_on_submit.get() {
                        self.set_draft_from_panel(String::new());
                    }
                    return true;
                }
                if self.on_submit.is_none() {
                    return false;
                }
                if self.streaming.get() || !self.queued_responses.get().is_empty() {
                    self.queue_response(ChatInputResponse::Text(text.clone()));
                    if self.clear_on_submit.get() {
                        self.set_draft_from_panel(String::new());
                    }
                    if self.streaming.get() {
                        return true;
                    }
                    return self.emit_next_queued_response();
                }
                self.dispatch_response(ChatInputResponse::Text(text.clone()));
                if self.clear_on_submit.get() {
                    self.set_draft_from_panel(String::new());
                }
                true
            }
            ChatInputMode::Choice(cfg) => {
                let Some(cb) = &self.on_submit else {
                    return false;
                };
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
                let Some(cb) = &self.on_submit else {
                    return false;
                };
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
            "slash_commands",
            "mention_candidates",
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
            "slash_commands" => Some(chat_slash_commands_to_component_value(
                &self.slash_commands.get(),
            )),
            "mention_candidates" => Some(chat_mention_candidates_to_component_value(
                &self.mention_candidates.get(),
            )),
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
                self.set_draft_from_panel(draft);
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
            "slash_commands" => {
                let commands = parse_chat_slash_commands_value(&value)
                    .map_err(|_| ComponentError::invalid_value(name, "slash command list"))?;
                self.slash_commands.set(commands);
                Ok(())
            }
            "mention_candidates" => {
                let candidates = parse_chat_mention_candidates_value(&value)
                    .map_err(|_| ComponentError::invalid_value(name, "mention candidate list"))?;
                self.mention_candidates.set(candidates);
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
        let indicator = self.queue_indicator_text();
        let indicator_height = u16::from(indicator.is_some() && area.height > 0);
        let input_area = Rect {
            height: area.height.saturating_sub(indicator_height),
            ..area
        };
        match &mut self.view {
            ChatInputView::Text(view) if input_area.height > 0 => view.draw(frame, input_area, ctx),
            ChatInputView::Choice(view) if input_area.height > 0 => {
                view.draw(frame, input_area, ctx)
            }
            ChatInputView::Confirm(view) if input_area.height > 0 => {
                view.draw(frame, input_area, ctx)
            }
            ChatInputView::Custom(view) if input_area.height > 0 => {
                view.draw(frame, input_area, ctx)
            }
            _ => {}
        }
        if let Some(indicator) = indicator {
            let status_area = Rect {
                y: area.y.saturating_add(input_area.height),
                height: indicator_height,
                ..area
            };
            if status_area.height > 0 {
                frame.render_widget(
                    Paragraph::new(Line::from(indicator)).style(Style::default().fg(Color::Yellow)),
                    status_area,
                );
            }
        }
        self.sync_completions(Some(input_area));
        self.slash_popup.draw(frame, frame.area(), ctx);
        self.mention_popup.draw(frame, frame.area(), ctx);
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
        let input_height: u16 = match self.mode.get() {
            ChatInputMode::Text(_) => 3,
            _ => 3,
        };
        input_height.saturating_add(u16::from(self.queue_indicator_text().is_some()))
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
        Some(
            self.estimated_height_for_mode(&self.mode.get())
                .saturating_add(u16::from(self.queue_indicator_text().is_some())),
        )
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
        self.sync_completions(None);
        if self.mention_open.get() {
            let popup_res = self.mention_popup.handle_event(event, ctx);
            if popup_res.is_consumed() {
                if matches!(
                    event,
                    Event::Key(KeyEvent {
                        code: KeyCode::Esc,
                        kind,
                        ..
                    }) if !matches!(kind, KeyEventKind::Release)
                ) {
                    self.dismiss_mention_completion_for_current_context();
                } else {
                    let _ = self.apply_accepted_mention();
                }
                return popup_res;
            }
        }
        if self.slash_open.get() {
            let popup_res = self.slash_popup.handle_event(event, ctx);
            if popup_res.is_consumed() {
                if matches!(
                    event,
                    Event::Key(KeyEvent {
                        code: KeyCode::Esc,
                        kind,
                        ..
                    }) if !matches!(kind, KeyEventKind::Release)
                ) {
                    self.dismiss_slash_completion_for_current_draft();
                } else {
                    let _ = self.apply_accepted_slash_command();
                }
                return popup_res;
            }
        }

        if let Event::Paste(text) = event
            && matches!(self.mode.get(), ChatInputMode::Text(_))
        {
            let res = self.handle_text_paste(text);
            self.sync_completions(None);
            return res;
        }

        let res = match &mut self.view {
            ChatInputView::Text(view) => view.handle_event(event, ctx),
            ChatInputView::Choice(view) => view.handle_event(event, ctx),
            ChatInputView::Confirm(view) => view.handle_event(event, ctx),
            ChatInputView::Custom(view) => view.handle_event(event, ctx),
        };
        self.sync_completions(None);

        if matches!(res.action, atto_ui::composable::ComponentAction::Submitted) {
            let _ = self.emit_response();
        }

        if !res.is_consumed()
            && is_escape_press(event)
            && self
                .on_streaming_interrupt
                .as_ref()
                .is_some_and(|callback| callback())
        {
            return EventResult::changed();
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

fn is_escape_press(event: &Event) -> bool {
    matches!(
        event,
        Event::Key(KeyEvent {
            code: KeyCode::Esc,
            kind,
            ..
        }) if !matches!(kind, KeyEventKind::Release)
    )
}

fn button_width(label: &str) -> u16 {
    let text_w = label.width().min(u16::MAX as usize) as u16;
    text_w.saturating_add(4).max(3)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use atto_ui::composable::{
        Component, ComponentContext, EventHandling, MouseCoordinateSpace, ScrollbarHost, TabMode,
    };
    use atto_ui::theme::Theme;
    use atto_ui::wm::WindowId;
    use crossterm::event::KeyModifiers;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    fn context(theme: &Theme) -> ComponentContext<'_> {
        ComponentContext {
            theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            drag: None,
        }
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn type_text(panel: &mut ChatInputPanel, theme: &Theme, text: &str) {
        for ch in text.chars() {
            panel.handle_event(&key(KeyCode::Char(ch)), context(theme));
        }
    }

    fn paste_text(panel: &mut ChatInputPanel, theme: &Theme, text: &str) -> EventResult {
        panel.handle_event(&Event::Paste(text.to_string()), context(theme))
    }

    fn draw_panel(panel: &mut ChatInputPanel, width: u16, height: u16) -> Vec<String> {
        let theme = Theme::dark();
        let ctx = context(&theme);
        let backend = TestBackend::new(width.max(1), height.max(1));
        let mut terminal = Terminal::new(backend).expect("terminal");
        let input_height = 5;
        let input_area = Rect::new(
            0,
            height.saturating_sub(input_height),
            width,
            input_height.min(height),
        );
        terminal
            .draw(|frame| panel.draw(frame, input_area, ctx))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let mut lines = Vec::new();
        for y in 0..height {
            let mut line = String::new();
            for x in 0..width {
                line.push_str(buffer.cell((x, y)).expect("cell").symbol());
            }
            lines.push(line);
        }
        lines
    }

    fn panel_with_commands(commands: Vec<ChatSlashCommand>) -> (ChatInputHandle, ChatInputPanel) {
        let handle = ChatInputHandle::new();
        handle.set_slash_commands(commands);
        let panel = handle.panel();
        (handle, panel)
    }

    fn panel_with_mentions(
        candidates: Vec<ChatMentionCandidate>,
    ) -> (ChatInputHandle, ChatInputPanel) {
        let handle = ChatInputHandle::new();
        handle.set_mention_candidates(candidates);
        let panel = handle.panel();
        (handle, panel)
    }

    #[test]
    fn slash_query_requires_line_start_command() {
        assert_eq!(slash_query_from_draft("/"), Some(String::new()));
        assert_eq!(slash_query_from_draft("/model"), Some("model".to_string()));
        assert_eq!(slash_query_from_draft("hello /model"), None);
        assert_eq!(slash_query_from_draft("/model\nnext"), None);
    }

    #[test]
    fn slash_popup_opens_and_filters_as_input_changes() {
        let (_handle, mut panel) = panel_with_commands(vec![
            ChatSlashCommand::new("/clear"),
            ChatSlashCommand::new("/model").detail("Switch model"),
        ]);
        let theme = Theme::dark();

        type_text(&mut panel, &theme, "/m");

        assert!(panel.slash_open.get());
        assert_eq!(panel.slash_query.get(), "m");
        let lines = draw_panel(&mut panel, 40, 12);
        assert!(lines.iter().any(|line| line.contains("/model")));
        assert!(!lines.iter().any(|line| line.contains("/clear")));
    }

    #[test]
    fn accepting_insert_command_writes_replacement_to_draft() {
        let (handle, mut panel) =
            panel_with_commands(vec![ChatSlashCommand::new("/model").replacement("/model ")]);
        let theme = Theme::dark();

        type_text(&mut panel, &theme, "/m");
        let result = panel.handle_event(&key(KeyCode::Enter), context(&theme));

        assert_eq!(result, EventResult::submitted());
        assert_eq!(handle.draft_binding().get(), "/model ");
        assert!(!panel.slash_open.get());
        panel.sync_slash_completion(None);
        assert!(!panel.slash_open.get());
    }

    #[test]
    fn accepting_insert_command_syncs_textarea_buffer_for_next_typing() {
        let (handle, mut panel) =
            panel_with_commands(vec![ChatSlashCommand::new("/model").replacement("/model ")]);
        let theme = Theme::dark();

        type_text(&mut panel, &theme, "/m");
        panel.handle_event(&key(KeyCode::Enter), context(&theme));
        type_text(&mut panel, &theme, "x");

        assert_eq!(handle.draft_binding().get(), "/model x");
    }

    #[test]
    fn accepting_submit_command_triggers_callback_and_clears_draft() {
        let handle = ChatInputHandle::new();
        handle.set_slash_commands(vec![ChatSlashCommand::new("/clear").submit_on_accept()]);
        let accepted = Arc::new(Mutex::new(Vec::<String>::new()));
        let accepted_for_callback = accepted.clone();
        let mut panel = handle.panel().on_slash_command(move |command| {
            accepted_for_callback.lock().unwrap().push(command.id);
        });
        let theme = Theme::dark();

        type_text(&mut panel, &theme, "/c");
        let result = panel.handle_event(&key(KeyCode::Enter), context(&theme));

        assert_eq!(result, EventResult::submitted());
        assert_eq!(accepted.lock().unwrap().as_slice(), ["clear"]);
        assert_eq!(handle.draft_binding().get(), "");
        assert!(!panel.slash_open.get());
    }

    #[test]
    fn escape_dismisses_until_draft_changes() {
        let (_handle, mut panel) = panel_with_commands(vec![ChatSlashCommand::new("/model")]);
        let theme = Theme::dark();

        type_text(&mut panel, &theme, "/");
        assert!(panel.slash_open.get());

        let result = panel.handle_event(&key(KeyCode::Esc), context(&theme));
        assert_eq!(result, EventResult::consumed());
        assert!(!panel.slash_open.get());

        panel.sync_slash_completion(None);
        assert!(!panel.slash_open.get());

        type_text(&mut panel, &theme, "m");
        assert!(panel.slash_open.get());
        assert_eq!(panel.slash_query.get(), "m");
    }

    #[test]
    fn escape_interrupts_streaming_after_input_ignores_it() {
        let handle = ChatInputHandle::new();
        let interrupts = Arc::new(Mutex::new(0usize));
        let interrupts_for_callback = interrupts.clone();
        let mut panel = handle.panel().on_streaming_interrupt(move || {
            let mut count = interrupts_for_callback.lock().expect("interrupt lock");
            *count += 1;
            true
        });
        let theme = Theme::dark();

        let result = panel.handle_event(&key(KeyCode::Esc), context(&theme));

        assert_eq!(result, EventResult::changed());
        assert_eq!(*interrupts.lock().expect("interrupt lock"), 1);
    }

    #[test]
    fn popup_escape_takes_priority_over_streaming_interrupt() {
        let (_handle, mut panel) = panel_with_commands(vec![ChatSlashCommand::new("/model")]);
        let interrupts = Arc::new(Mutex::new(0usize));
        let interrupts_for_callback = interrupts.clone();
        panel = panel.on_streaming_interrupt(move || {
            let mut count = interrupts_for_callback.lock().expect("interrupt lock");
            *count += 1;
            true
        });
        let theme = Theme::dark();

        type_text(&mut panel, &theme, "/");
        assert!(panel.slash_open.get());
        let result = panel.handle_event(&key(KeyCode::Esc), context(&theme));

        assert_eq!(result, EventResult::consumed());
        assert_eq!(*interrupts.lock().expect("interrupt lock"), 0);
        assert!(!panel.slash_open.get());
    }

    #[test]
    fn mention_popup_escape_takes_priority_over_streaming_interrupt() {
        let (_handle, mut panel) =
            panel_with_mentions(vec![ChatMentionCandidate::new("Cargo.toml")]);
        let interrupts = Arc::new(Mutex::new(0usize));
        let interrupts_for_callback = interrupts.clone();
        panel = panel.on_streaming_interrupt(move || {
            let mut count = interrupts_for_callback.lock().expect("interrupt lock");
            *count += 1;
            true
        });
        let theme = Theme::dark();

        type_text(&mut panel, &theme, "please @ca");
        assert!(panel.mention_open.get());
        let result = panel.handle_event(&key(KeyCode::Esc), context(&theme));

        assert_eq!(result, EventResult::consumed());
        assert_eq!(*interrupts.lock().expect("interrupt lock"), 0);
        assert!(!panel.mention_open.get());
    }

    #[test]
    fn escape_is_ignored_when_streaming_interrupt_declines() {
        let handle = ChatInputHandle::new();
        let mut panel = handle.panel().on_streaming_interrupt(|| false);
        let theme = Theme::dark();

        let result = panel.handle_event(&key(KeyCode::Esc), context(&theme));

        assert_eq!(result, EventResult::ignored());
    }

    #[test]
    fn multiline_paste_normalization_preserves_body_and_trims_blank_tail() {
        assert_eq!(
            normalize_chat_text_paste("first\r\nsecond\rthird\n\n\t \n"),
            "first\nsecond\nthird"
        );
        assert_eq!(
            normalize_chat_text_paste("first\n\n  indented"),
            "first\n\n  indented"
        );
        assert_eq!(normalize_chat_text_paste("single line  "), "single line  ");
        assert_eq!(
            normalize_chat_text_paste("\u{1b}[200~first\r\nsecond\n\u{1b}[201~"),
            "first\nsecond"
        );
    }

    #[test]
    fn pasting_multiline_text_updates_textarea_buffer_for_next_typing() {
        let handle = ChatInputHandle::new();
        let mut panel = handle.panel();
        let theme = Theme::dark();

        let result = paste_text(
            &mut panel,
            &theme,
            "\u{1b}[200~first\r\nsecond\rthird\n\n\u{1b}[201~",
        );
        type_text(&mut panel, &theme, "!");

        assert_eq!(result, EventResult::changed());
        assert_eq!(handle.draft_binding().get(), "first\nsecond\nthird!");
    }

    #[test]
    fn submitting_multiline_paste_emits_normalized_text() {
        let handle = ChatInputHandle::new();
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let submitted_for_callback = submitted.clone();
        let mut panel = handle.panel().on_submit(move |response| {
            submitted_for_callback.lock().unwrap().push(response);
        });
        let theme = Theme::dark();

        paste_text(&mut panel, &theme, "alpha\r\nbeta\n\n");
        let result = panel.handle_event(&key(KeyCode::Enter), context(&theme));

        assert_eq!(result, EventResult::submitted());
        assert_eq!(
            *submitted.lock().unwrap(),
            vec![ChatInputResponse::Text("alpha\nbeta".to_string())]
        );
        assert_eq!(
            handle.history_binding().get(),
            vec!["alpha\nbeta".to_string()]
        );
    }

    #[test]
    fn register_slash_command_replaces_existing_id() {
        let handle = ChatInputHandle::new();
        handle.set_slash_commands(vec![ChatSlashCommand::with_id("model", "/model")]);

        handle.register_slash_command(
            ChatSlashCommand::with_id("model", "/model")
                .detail("Choose a model")
                .replacement("/model "),
        );

        let commands = handle.slash_commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].detail.as_deref(), Some("Choose a model"));
        assert_eq!(commands[0].replacement, "/model ");
    }

    #[test]
    fn mention_query_uses_token_at_cursor() {
        let draft = "open @src/lib.rs and @Cargo.toml";
        let first_cursor = "open @sr".len();
        let first = mention_query_from_draft_at(draft, first_cursor).expect("first mention");
        assert_eq!(first.query, "sr");
        assert_eq!(first.replacement_start, "open ".len());
        assert_eq!(first.replacement_end, "open @src/lib.rs".len());

        let second = mention_query_from_draft_at(draft, draft.len()).expect("second mention");
        assert_eq!(second.query, "Cargo.toml");
        assert_eq!(
            second.replacement_start,
            draft.rfind('@').expect("second @")
        );
        assert_eq!(second.replacement_end, draft.len());

        assert!(
            mention_query_from_draft_at("mail me@example.com", "mail me@example.com".len())
                .is_none()
        );
        assert!(mention_query_from_draft_at("@wide/你好", "@wide/你".len()).is_some());
    }

    #[test]
    fn mention_popup_uses_provider_context_and_filters() {
        let handle = ChatInputHandle::new();
        let seen = Arc::new(Mutex::new(Vec::<ChatMentionContext>::new()));
        let seen_for_provider = seen.clone();
        let mut panel = handle.panel().mention_provider(move |context| {
            seen_for_provider.lock().unwrap().push(context);
            vec![
                ChatMentionCandidate::new("Cargo.toml").detail("file"),
                ChatMentionCandidate::new("src/lib.rs").detail("file"),
            ]
        });
        let theme = Theme::dark();

        type_text(&mut panel, &theme, "please @ca");

        assert!(panel.mention_open.get());
        assert_eq!(panel.mention_query.get(), "ca");
        assert_eq!(seen.lock().unwrap().last().expect("context").query, "ca");
        let lines = draw_panel(&mut panel, 48, 12);
        assert!(lines.iter().any(|line| line.contains("Cargo.toml")));
        assert!(!lines.iter().any(|line| line.contains("src/lib.rs")));
    }

    #[test]
    fn accepting_mention_replaces_current_token() {
        let (handle, mut panel) =
            panel_with_mentions(vec![ChatMentionCandidate::new("Cargo.toml")]);
        let theme = Theme::dark();

        type_text(&mut panel, &theme, "please @ca");
        let result = panel.handle_event(&key(KeyCode::Enter), context(&theme));

        assert_eq!(result, EventResult::submitted());
        assert_eq!(handle.draft_binding().get(), "please @Cargo.toml");
        assert!(!panel.mention_open.get());
        panel.sync_mention_completion(None);
        assert!(!panel.mention_open.get());
    }

    #[test]
    fn accepting_mention_replaces_token_at_cursor_without_touching_later_mentions() {
        let (handle, mut panel) = panel_with_mentions(vec![
            ChatMentionCandidate::new("src/lib.rs"),
            ChatMentionCandidate::new("Cargo.toml"),
        ]);
        let theme = Theme::dark();

        type_text(&mut panel, &theme, "@sr and @ca");
        match &mut panel.view {
            ChatInputView::Text(view) => view.set_cursor_byte_index("@sr".len()),
            _ => panic!("expected text view"),
        }
        panel.sync_completions(None);

        assert!(panel.mention_open.get());
        assert_eq!(panel.mention_query.get(), "sr");
        let result = panel.handle_event(&key(KeyCode::Enter), context(&theme));

        assert_eq!(result, EventResult::submitted());
        assert_eq!(handle.draft_binding().get(), "@src/lib.rs and @ca");
    }

    #[test]
    fn mention_does_not_open_without_source_or_inside_email() {
        let (_handle, mut panel) = panel_with_mentions(Vec::new());
        let theme = Theme::dark();

        type_text(&mut panel, &theme, "hello @");

        assert!(!panel.mention_open.get());

        let (_handle, mut panel) = panel_with_mentions(vec![ChatMentionCandidate::new("example")]);
        type_text(&mut panel, &theme, "me@example.com");

        assert!(!panel.mention_open.get());
        assert!(panel.mention_active.is_none());
    }

    #[test]
    fn register_mention_candidate_replaces_existing_id() {
        let handle = ChatInputHandle::new();
        handle.set_mention_candidates(vec![ChatMentionCandidate::with_id("cargo", "Cargo.toml")]);

        handle.register_mention_candidate(
            ChatMentionCandidate::with_id("cargo", "Cargo.toml")
                .detail("manifest")
                .replacement("@Cargo.toml "),
        );

        let candidates = handle.mention_candidates();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].detail.as_deref(), Some("manifest"));
        assert_eq!(candidates[0].replacement, "@Cargo.toml ");
    }

    #[test]
    fn streaming_text_submit_queues_and_shows_status() {
        let handle = ChatInputHandle::new();
        handle.streaming_binding().set(true);
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let submitted_for_callback = submitted.clone();
        let mut panel = handle.panel().on_submit(move |response| {
            submitted_for_callback.lock().unwrap().push(response);
        });
        let theme = Theme::dark();

        type_text(&mut panel, &theme, "queued one");
        let result = panel.handle_event(&key(KeyCode::Enter), context(&theme));

        assert_eq!(result, EventResult::submitted());
        assert!(submitted.lock().unwrap().is_empty());
        assert_eq!(handle.draft_binding().get(), "");
        assert_eq!(
            handle.queued_responses(),
            vec![ChatInputResponse::Text("queued one".to_string())]
        );
        let lines = draw_panel(&mut panel, 64, 8);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Queued 1 message while streaming"))
        );
    }

    #[test]
    fn queued_text_sends_after_streaming_finishes() {
        let handle = ChatInputHandle::new();
        handle.streaming_binding().set(true);
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let submitted_for_callback = submitted.clone();
        let mut panel = handle.panel().on_submit(move |response| {
            submitted_for_callback.lock().unwrap().push(response);
        });
        let theme = Theme::dark();

        type_text(&mut panel, &theme, "first");
        panel.handle_event(&key(KeyCode::Enter), context(&theme));
        type_text(&mut panel, &theme, "second");
        panel.handle_event(&key(KeyCode::Enter), context(&theme));
        handle.streaming_binding().set(false);

        assert!(submitted.lock().unwrap().is_empty());
        let lines = draw_panel(&mut panel, 64, 8);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Queued 2 messages; press Enter to send next"))
        );

        panel.handle_event(&key(KeyCode::Enter), context(&theme));
        assert_eq!(
            *submitted.lock().unwrap(),
            vec![ChatInputResponse::Text("first".to_string())]
        );
        assert_eq!(
            handle.queued_responses(),
            vec![ChatInputResponse::Text("second".to_string())]
        );

        panel.handle_event(&key(KeyCode::Enter), context(&theme));
        assert_eq!(
            *submitted.lock().unwrap(),
            vec![
                ChatInputResponse::Text("first".to_string()),
                ChatInputResponse::Text("second".to_string())
            ]
        );
        assert!(handle.queued_responses().is_empty());
    }

    #[test]
    fn new_draft_after_streaming_preserves_queued_fifo_order() {
        let handle = ChatInputHandle::new();
        handle.streaming_binding().set(true);
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let submitted_for_callback = submitted.clone();
        let mut panel = handle.panel().on_submit(move |response| {
            submitted_for_callback.lock().unwrap().push(response);
        });
        let theme = Theme::dark();

        type_text(&mut panel, &theme, "first");
        panel.handle_event(&key(KeyCode::Enter), context(&theme));
        handle.streaming_binding().set(false);
        type_text(&mut panel, &theme, "second");
        panel.handle_event(&key(KeyCode::Enter), context(&theme));

        assert_eq!(
            *submitted.lock().unwrap(),
            vec![ChatInputResponse::Text("first".to_string())]
        );
        assert_eq!(
            handle.queued_responses(),
            vec![ChatInputResponse::Text("second".to_string())]
        );
        assert_eq!(handle.draft_binding().get(), "");
    }

    #[test]
    fn text_submit_interceptor_runs_before_streaming_queue() {
        let handle = ChatInputHandle::new();
        handle.streaming_binding().set(true);
        let intercepted = Arc::new(Mutex::new(Vec::new()));
        let intercepted_for_callback = intercepted.clone();
        handle.set_text_submit_interceptor(ChatTextSubmitInterceptor::new(move |text| {
            intercepted_for_callback.lock().unwrap().push(text);
            true
        }));
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let submitted_for_callback = submitted.clone();
        let mut panel = handle.panel().on_submit(move |response| {
            submitted_for_callback.lock().unwrap().push(response);
        });
        let theme = Theme::dark();

        type_text(&mut panel, &theme, "edited prompt");
        panel.handle_event(&key(KeyCode::Enter), context(&theme));

        assert_eq!(
            *intercepted.lock().unwrap(),
            vec!["edited prompt".to_string()]
        );
        assert!(submitted.lock().unwrap().is_empty());
        assert!(handle.queued_responses().is_empty());
        assert_eq!(handle.draft_binding().get(), "");
    }
}
