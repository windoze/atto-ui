use std::env;
use std::io::{self, Write};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::clipboard::encode_base64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageProtocol {
    Kitty,
    Iterm2,
    Sixel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageData<'a> {
    Binary(&'a [u8]),
    Sixel(&'a str),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImageRenderOptions {
    pub width_cells: Option<u16>,
    pub height_cells: Option<u16>,
    pub name: Option<String>,
}

impl ImageRenderOptions {
    pub fn width_cells(mut self, width: u16) -> Self {
        self.width_cells = Some(width);
        self
    }

    pub fn height_cells(mut self, height: u16) -> Self {
        self.height_cells = Some(height);
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// Builds an OSC8 hyperlink sequence for terminals that support clickable links.
///
/// Unsupported terminals safely ignore OSC8 metadata and still display the label.
pub fn osc8_hyperlink_sequence(uri: &str, label: &str) -> String {
    let uri = strip_terminating_controls(uri);
    let label = strip_terminating_controls(label);
    format!("\x1b]8;;{uri}\x1b\\{label}\x1b]8;;\x1b\\")
}

pub fn write_osc8_hyperlink(mut writer: impl Write, uri: &str, label: &str) -> io::Result<()> {
    writer.write_all(osc8_hyperlink_sequence(uri, label).as_bytes())?;
    writer.flush()
}

/// Detects the best supported inline-image protocol from common terminal environment variables.
pub fn detect_image_protocol() -> Option<ImageProtocol> {
    detect_image_protocol_from_env(|name| env::var(name).ok())
}

pub fn detect_image_protocol_from_env(
    mut get_env: impl FnMut(&str) -> Option<String>,
) -> Option<ImageProtocol> {
    if get_env("KITTY_WINDOW_ID").is_some() {
        return Some(ImageProtocol::Kitty);
    }

    if get_env("TERM_PROGRAM").as_deref() == Some("iTerm.app") {
        return Some(ImageProtocol::Iterm2);
    }

    let term = get_env("TERM").unwrap_or_default().to_ascii_lowercase();
    if term.contains("sixel") || term.contains("mlterm") {
        return Some(ImageProtocol::Sixel);
    }

    None
}

/// Builds an inline-image control sequence for the selected protocol.
///
/// Sixel requires already-encoded sixel payload (`ImageData::Sixel`); binary raster encoding is
/// intentionally left to callers so core remains dependency-light and std-only.
pub fn terminal_image_sequence(
    protocol: ImageProtocol,
    data: ImageData<'_>,
    options: &ImageRenderOptions,
) -> Option<String> {
    match protocol {
        ImageProtocol::Kitty => Some(kitty_image_sequence(data, options)),
        ImageProtocol::Iterm2 => Some(iterm2_image_sequence(data, options)),
        ImageProtocol::Sixel => match data {
            ImageData::Sixel(payload) => Some(format!("\x1bPq{payload}\x1b\\")),
            ImageData::Binary(_) => None,
        },
    }
}

pub fn image_sequence_or_fallback(
    protocol: Option<ImageProtocol>,
    data: ImageData<'_>,
    options: &ImageRenderOptions,
    fallback: impl Into<String>,
) -> String {
    protocol
        .and_then(|protocol| terminal_image_sequence(protocol, data, options))
        .unwrap_or_else(|| fallback.into())
}

pub(crate) fn draw_shadow(buf: &mut Buffer, rect: Rect, bounds: Rect, style: Style) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    let style = Style::reset().patch(style);
    let shadow_x = rect.x.saturating_add(rect.width);
    let shadow_y = rect.y.saturating_add(rect.height);

    if shadow_x < bounds.x.saturating_add(bounds.width) {
        for y in rect.y.saturating_add(1)..rect.y.saturating_add(rect.height) {
            if y >= bounds.y.saturating_add(bounds.height) {
                break;
            }
            if shadow_x < bounds.x || y < bounds.y {
                continue;
            }
            if let Some(cell) = buf.cell_mut((shadow_x, y)) {
                cell.set_symbol(" ");
                cell.set_style(style);
            }
        }
    }

    if shadow_y < bounds.y.saturating_add(bounds.height) {
        for x in rect.x.saturating_add(1)..rect.x.saturating_add(rect.width) {
            if x >= bounds.x.saturating_add(bounds.width) {
                break;
            }
            if x < bounds.x || shadow_y < bounds.y {
                continue;
            }
            if let Some(cell) = buf.cell_mut((x, shadow_y)) {
                cell.set_symbol(" ");
                cell.set_style(style);
            }
        }
    }

    if shadow_x < bounds.x.saturating_add(bounds.width)
        && shadow_y < bounds.y.saturating_add(bounds.height)
        && shadow_x >= bounds.x
        && shadow_y >= bounds.y
        && let Some(cell) = buf.cell_mut((shadow_x, shadow_y))
    {
        cell.set_symbol(" ");
        cell.set_style(style);
    }
}

fn kitty_image_sequence(data: ImageData<'_>, options: &ImageRenderOptions) -> String {
    let bytes = image_data_bytes(data);
    let mut params = vec!["a=T".to_string(), "f=100".to_string()];
    if let Some(width) = options.width_cells {
        params.push(format!("c={width}"));
    }
    if let Some(height) = options.height_cells {
        params.push(format!("r={height}"));
    }
    if let Some(name) = &options.name {
        params.push(format!("i={}", stable_image_id(name)));
    }
    format!("\x1b_G{};{}\x1b\\", params.join(","), encode_base64(bytes))
}

fn iterm2_image_sequence(data: ImageData<'_>, options: &ImageRenderOptions) -> String {
    let bytes = image_data_bytes(data);
    let mut params = vec!["inline=1".to_string()];
    if let Some(width) = options.width_cells {
        params.push(format!("width={width}"));
    }
    if let Some(height) = options.height_cells {
        params.push(format!("height={height}"));
    }
    if let Some(name) = &options.name {
        params.push(format!("name={}", encode_base64(name.as_bytes())));
    }
    format!(
        "\x1b]1337;File={}:{}\x07",
        params.join(";"),
        encode_base64(bytes)
    )
}

fn image_data_bytes(data: ImageData<'_>) -> &[u8] {
    match data {
        ImageData::Binary(bytes) => bytes,
        ImageData::Sixel(payload) => payload.as_bytes(),
    }
}

fn strip_terminating_controls(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !matches!(*ch, '\x1b' | '\x07'))
        .collect()
}

fn stable_image_id(name: &str) -> u32 {
    name.bytes().fold(0x811c_9dc5u32, |hash, byte| {
        hash.wrapping_mul(16_777_619) ^ byte as u32
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc8_sequence_wraps_visible_label() {
        assert_eq!(
            osc8_hyperlink_sequence("https://example.test", "Open"),
            "\x1b]8;;https://example.test\x1b\\Open\x1b]8;;\x1b\\"
        );
    }

    #[test]
    fn image_protocol_detection_prefers_explicit_terminals() {
        assert_eq!(
            detect_image_protocol_from_env(|name| match name {
                "KITTY_WINDOW_ID" => Some("1".to_string()),
                _ => None,
            }),
            Some(ImageProtocol::Kitty)
        );
        assert_eq!(
            detect_image_protocol_from_env(|name| match name {
                "TERM_PROGRAM" => Some("iTerm.app".to_string()),
                _ => None,
            }),
            Some(ImageProtocol::Iterm2)
        );
        assert_eq!(
            detect_image_protocol_from_env(|name| match name {
                "TERM" => Some("xterm-sixel".to_string()),
                _ => None,
            }),
            Some(ImageProtocol::Sixel)
        );
    }

    #[test]
    fn image_sequence_falls_back_without_protocol_or_sixel_payload() {
        let options = ImageRenderOptions::default()
            .width_cells(10)
            .height_cells(4);

        assert_eq!(
            image_sequence_or_fallback(None, ImageData::Binary(b"png"), &options, "[image]"),
            "[image]"
        );
        assert_eq!(
            image_sequence_or_fallback(
                Some(ImageProtocol::Sixel),
                ImageData::Binary(b"png"),
                &options,
                "[image]"
            ),
            "[image]"
        );
    }

    #[test]
    fn kitty_and_iterm_sequences_base64_encode_binary_data() {
        let options = ImageRenderOptions::default().width_cells(2).height_cells(1);

        assert_eq!(
            terminal_image_sequence(ImageProtocol::Kitty, ImageData::Binary(b"png"), &options)
                .as_deref(),
            Some("\x1b_Ga=T,f=100,c=2,r=1;cG5n\x1b\\")
        );
        assert_eq!(
            terminal_image_sequence(ImageProtocol::Iterm2, ImageData::Binary(b"png"), &options)
                .as_deref(),
            Some("\x1b]1337;File=inline=1;width=2;height=1:cG5n\x07")
        );
    }
}
