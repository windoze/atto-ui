//! JavaScript event decoding for `AppHost::sendEvent`.

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use napi::Result;
use serde_json::{Map, Value};

use crate::error::invalid_arg;

const JS_MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// Decode the JavaScript event shape used by Node tests and React integration.
pub fn event_from_json(value: Value) -> Result<Event> {
    match value {
        Value::String(name) => Ok(Event::Key(KeyEvent::new(
            key_code_from_name(&name)?,
            KeyModifiers::NONE,
        ))),
        Value::Object(object) => event_from_object(&object),
        _ => Err(invalid_arg("event must be a string or object")),
    }
}

fn event_from_object(object: &Map<String, Value>) -> Result<Event> {
    let kind = expect_string(
        expect_any_field(object, &["type", "event"], "event type")?,
        "event type",
    )?;
    match normalize_name(&kind).as_str() {
        "key" => key_event_from_object(object).map(Event::Key),
        "mouse" => mouse_event_from_object(object).map(Event::Mouse),
        "paste" => Ok(Event::Paste(expect_string(
            expect_field(object, "text", "paste text")?,
            "paste text",
        )?)),
        "resize" => Ok(Event::Resize(
            u16_from_value(
                expect_any_field(object, &["cols", "columns"], "resize cols")?,
                "resize cols",
            )?,
            u16_from_value(
                expect_any_field(object, &["rows", "height"], "resize rows")?,
                "resize rows",
            )?,
        )),
        "focusgained" => Ok(Event::FocusGained),
        "focuslost" => Ok(Event::FocusLost),
        _ => Err(invalid_arg(format!("unknown event type: {kind}"))),
    }
}

fn key_event_from_object(object: &Map<String, Value>) -> Result<KeyEvent> {
    let code = if let Some(value) = get_field(object, "char") {
        key_code_from_char_value(value)?
    } else {
        let key = expect_string(
            expect_field(object, "key", "key event key")?,
            "key event key",
        )?;
        key_code_from_name(&key)?
    };
    let modifiers = get_field(object, "modifiers")
        .map(key_modifiers_from_value)
        .transpose()?
        .unwrap_or(KeyModifiers::NONE);
    let kind = get_field(object, "kind")
        .map(key_event_kind_from_value)
        .transpose()?
        .unwrap_or(KeyEventKind::Press);

    Ok(KeyEvent {
        code,
        modifiers,
        kind,
        state: KeyEventState::empty(),
    })
}

fn key_code_from_char_value(value: &Value) -> Result<KeyCode> {
    let value = expect_string(value, "key char")?;
    let mut chars = value.chars();
    let Some(ch) = chars.next() else {
        return Err(invalid_arg("key char must not be empty"));
    };
    if chars.next().is_some() {
        return Err(invalid_arg("key char must contain exactly one character"));
    }
    Ok(KeyCode::Char(ch))
}

fn key_code_from_name(name: &str) -> Result<KeyCode> {
    let normalized = normalize_name(name);
    match normalized.as_str() {
        "backspace" => Ok(KeyCode::Backspace),
        "enter" | "return" => Ok(KeyCode::Enter),
        "left" => Ok(KeyCode::Left),
        "right" => Ok(KeyCode::Right),
        "up" => Ok(KeyCode::Up),
        "down" => Ok(KeyCode::Down),
        "home" => Ok(KeyCode::Home),
        "end" => Ok(KeyCode::End),
        "pageup" => Ok(KeyCode::PageUp),
        "pagedown" => Ok(KeyCode::PageDown),
        "tab" => Ok(KeyCode::Tab),
        "backtab" => Ok(KeyCode::BackTab),
        "delete" | "del" => Ok(KeyCode::Delete),
        "insert" | "ins" => Ok(KeyCode::Insert),
        "esc" | "escape" => Ok(KeyCode::Esc),
        value if value.starts_with('f') => {
            let n = value[1..]
                .parse::<u8>()
                .map_err(|_| invalid_arg(format!("invalid function key: {name}")))?;
            Ok(KeyCode::F(n))
        }
        value => {
            let mut chars = value.chars();
            if let Some(ch) = chars.next()
                && chars.next().is_none()
            {
                return Ok(KeyCode::Char(ch));
            }
            Err(invalid_arg(format!("unknown key: {name}")))
        }
    }
}

fn key_event_kind_from_value(value: &Value) -> Result<KeyEventKind> {
    let value = expect_string(value, "key event kind")?;
    match normalize_name(&value).as_str() {
        "press" | "down" => Ok(KeyEventKind::Press),
        "release" | "up" => Ok(KeyEventKind::Release),
        "repeat" => Ok(KeyEventKind::Repeat),
        _ => Err(invalid_arg("invalid key event kind")),
    }
}

fn mouse_event_from_object(object: &Map<String, Value>) -> Result<MouseEvent> {
    let kind_name = expect_string(
        expect_field(object, "kind", "mouse event kind")?,
        "mouse event kind",
    )?;
    let kind = mouse_event_kind_from_name(&kind_name, object)?;
    let column = u16_from_value(
        expect_any_field(object, &["column", "x"], "mouse column")?,
        "mouse column",
    )?;
    let row = u16_from_value(
        expect_any_field(object, &["row", "y"], "mouse row")?,
        "mouse row",
    )?;
    let modifiers = get_field(object, "modifiers")
        .map(key_modifiers_from_value)
        .transpose()?
        .unwrap_or(KeyModifiers::NONE);

    Ok(MouseEvent {
        kind,
        column,
        row,
        modifiers,
    })
}

fn mouse_event_kind_from_name(name: &str, object: &Map<String, Value>) -> Result<MouseEventKind> {
    match normalize_name(name).as_str() {
        "down" => Ok(MouseEventKind::Down(mouse_button_from_object(object)?)),
        "up" => Ok(MouseEventKind::Up(mouse_button_from_object(object)?)),
        "drag" => Ok(MouseEventKind::Drag(mouse_button_from_object(object)?)),
        "move" | "moved" => Ok(MouseEventKind::Moved),
        "scrollup" => Ok(MouseEventKind::ScrollUp),
        "scrolldown" => Ok(MouseEventKind::ScrollDown),
        "scrollleft" => Ok(MouseEventKind::ScrollLeft),
        "scrollright" => Ok(MouseEventKind::ScrollRight),
        _ => Err(invalid_arg(format!("unknown mouse event kind: {name}"))),
    }
}

fn mouse_button_from_object(object: &Map<String, Value>) -> Result<MouseButton> {
    let value = get_field(object, "button")
        .map(|value| expect_string(value, "mouse button"))
        .transpose()?
        .unwrap_or_else(|| "left".to_string());
    match normalize_name(&value).as_str() {
        "left" => Ok(MouseButton::Left),
        "right" => Ok(MouseButton::Right),
        "middle" => Ok(MouseButton::Middle),
        _ => Err(invalid_arg(format!("unknown mouse button: {value}"))),
    }
}

fn key_modifiers_from_value(value: &Value) -> Result<KeyModifiers> {
    match value {
        Value::Null => Ok(KeyModifiers::NONE),
        Value::String(value) => key_modifiers_from_names(split_modifier_names(value)),
        Value::Array(values) => {
            let names = values
                .iter()
                .map(|value| expect_string(value, "modifier"))
                .collect::<Result<Vec<_>>>()?;
            key_modifiers_from_names(names)
        }
        _ => Err(invalid_arg("modifiers must be a string or array")),
    }
}

fn split_modifier_names(value: &str) -> Vec<String> {
    value
        .split(['+', '|', ',', ' '])
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn key_modifiers_from_names<I>(names: I) -> Result<KeyModifiers>
where
    I: IntoIterator<Item = String>,
{
    let mut modifiers = KeyModifiers::NONE;
    for name in names {
        match normalize_name(&name).as_str() {
            "shift" => modifiers |= KeyModifiers::SHIFT,
            "control" | "ctrl" => modifiers |= KeyModifiers::CONTROL,
            "alt" | "option" => modifiers |= KeyModifiers::ALT,
            "super" | "cmd" | "command" => modifiers |= KeyModifiers::SUPER,
            "hyper" => modifiers |= KeyModifiers::HYPER,
            "meta" => modifiers |= KeyModifiers::META,
            "none" | "" => {}
            _ => return Err(invalid_arg(format!("unknown modifier: {name}"))),
        }
    }
    Ok(modifiers)
}

fn expect_field<'a>(
    object: &'a Map<String, Value>,
    name: &str,
    context: &str,
) -> Result<&'a Value> {
    get_field(object, name).ok_or_else(|| invalid_arg(format!("{context} is required")))
}

fn expect_any_field<'a>(
    object: &'a Map<String, Value>,
    names: &[&str],
    context: &str,
) -> Result<&'a Value> {
    names
        .iter()
        .find_map(|name| get_field(object, name))
        .ok_or_else(|| invalid_arg(format!("{context} is required")))
}

fn get_field<'a>(object: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    object.get(name).filter(|value| !value.is_null())
}

fn expect_string(value: &Value, context: &str) -> Result<String> {
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| invalid_arg(format!("{context} must be a string")))
}

fn u16_from_value(value: &Value, context: &str) -> Result<u16> {
    let number = value
        .as_number()
        .ok_or_else(|| invalid_arg(format!("{context} must be a number")))?;
    if let Some(value) = number.as_u64() {
        return u16::try_from(value).map_err(|_| invalid_arg(format!("{context} must fit in u16")));
    }
    if let Some(value) = number.as_i64()
        && value >= 0
    {
        return u16::try_from(value as u64)
            .map_err(|_| invalid_arg(format!("{context} must fit in u16")));
    }
    if let Some(value) = number.as_f64()
        && value.is_finite()
        && value.fract() == 0.0
        && (0.0..=JS_MAX_SAFE_INTEGER).contains(&value)
    {
        return u16::try_from(value as u64)
            .map_err(|_| invalid_arg(format!("{context} must fit in u16")));
    }
    Err(invalid_arg(format!(
        "{context} must be a non-negative integer"
    )))
}

fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|ch| !matches!(*ch, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_key_events() {
        let event = event_from_json(json!({
            "type": "key",
            "key": "enter",
            "modifiers": ["ctrl", "shift"]
        }))
        .unwrap();

        let Event::Key(event) = event else {
            panic!("expected key event");
        };
        assert_eq!(event.code, KeyCode::Enter);
        assert!(event.modifiers.contains(KeyModifiers::CONTROL));
        assert!(event.modifiers.contains(KeyModifiers::SHIFT));
    }

    #[test]
    fn parses_mouse_events() {
        let event = event_from_json(json!({
            "type": "mouse",
            "kind": "down",
            "button": "left",
            "x": 3,
            "y": 4
        }))
        .unwrap();

        let Event::Mouse(event) = event else {
            panic!("expected mouse event");
        };
        assert_eq!(event.kind, MouseEventKind::Down(MouseButton::Left));
        assert_eq!(event.column, 3);
        assert_eq!(event.row, 4);
    }
}
