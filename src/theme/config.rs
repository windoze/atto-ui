use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result, anyhow};
use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThemeConfig {
    /// Optional preset used as the base before applying this file's overlays.
    #[serde(default, alias = "preset")]
    pub base: Option<String>,

    #[serde(default)]
    pub glyphs: HashMap<String, String>,

    /// Named foreground/background pairs.
    #[serde(default)]
    pub colors: HashMap<String, ColorSpec>,

    /// Named modifier lists (e.g. `["bold", "reverse"]`).
    #[serde(default)]
    pub styles: HashMap<String, Vec<String>>,

    /// Optional terminal-emulator color overrides.
    #[serde(default)]
    pub terminal: Option<TerminalThemeConfig>,
}

/// Serializable overrides for a theme's [`TerminalTheme`](super::TerminalTheme).
///
/// Every field is optional; only the ones present override the base theme's
/// terminal colors. Color strings accept the same syntax as [`ColorSpec`].
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TerminalThemeConfig {
    /// ANSI colors 0-15. When present, must contain all 16 entries.
    pub ansi: Option<Vec<String>>,
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub cursor: Option<String>,
    pub cursor_text: Option<String>,
    pub selection_bg: Option<String>,
    pub selection_fg: Option<String>,
}

impl TerminalThemeConfig {
    /// Applies these overrides onto `base`, resolving color strings.
    pub(super) fn apply_onto(&self, base: &mut super::TerminalTheme) -> Result<()> {
        if let Some(ansi) = &self.ansi {
            if ansi.len() != 16 {
                return Err(anyhow!(
                    "terminal.ansi must contain exactly 16 colors, got {}",
                    ansi.len()
                ));
            }
            for (i, spec) in ansi.iter().enumerate() {
                base.ansi[i] = parse_color(spec)
                    .with_context(|| format!("invalid terminal ANSI color {i}"))?;
            }
        }
        let set = |field: &mut Color, spec: &Option<String>, name: &str| -> Result<()> {
            if let Some(s) = spec {
                *field =
                    parse_color(s).with_context(|| format!("invalid terminal {name} color"))?;
            }
            Ok(())
        };
        set(&mut base.foreground, &self.foreground, "foreground")?;
        set(&mut base.background, &self.background, "background")?;
        set(&mut base.cursor, &self.cursor, "cursor")?;
        set(&mut base.cursor_text, &self.cursor_text, "cursor_text")?;
        set(&mut base.selection_bg, &self.selection_bg, "selection_bg")?;
        set(&mut base.selection_fg, &self.selection_fg, "selection_fg")?;
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ColorSpec {
    pub fg: Option<String>,
    pub bg: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeConfigFormat {
    Json,
    Yaml,
}

impl ThemeConfig {
    pub fn from_str(input: &str, format: ThemeConfigFormat) -> Result<Self> {
        match format {
            ThemeConfigFormat::Json => serde_json::from_str(input).context("parse theme JSON"),
            ThemeConfigFormat::Yaml => serde_yaml::from_str(input).context("parse theme YAML"),
        }
    }

    pub fn from_bytes(input: &[u8], format: ThemeConfigFormat) -> Result<Self> {
        match format {
            ThemeConfigFormat::Json => serde_json::from_slice(input).context("parse theme JSON"),
            ThemeConfigFormat::Yaml => serde_yaml::from_slice(input).context("parse theme YAML"),
        }
    }

    pub fn infer_format_from_path(path: &std::path::Path) -> Option<ThemeConfigFormat> {
        let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
        match ext.as_str() {
            "json" => Some(ThemeConfigFormat::Json),
            "yaml" | "yml" => Some(ThemeConfigFormat::Yaml),
            _ => None,
        }
    }

    pub fn from_bytes_infer(input: &[u8], path: Option<&std::path::Path>) -> Result<Self> {
        if let Some(path) = path
            && let Some(format) = Self::infer_format_from_path(path)
        {
            return Self::from_bytes(input, format)
                .with_context(|| format!("parse theme file {}", path.display()));
        }

        let json_err = match Self::from_bytes(input, ThemeConfigFormat::Json) {
            Ok(v) => return Ok(v),
            Err(e) => e,
        };
        match Self::from_bytes(input, ThemeConfigFormat::Yaml) {
            Ok(v) => Ok(v),
            Err(yaml_err) => Err(anyhow!(
                "failed to parse theme as JSON ({json_err}) or YAML ({yaml_err})"
            )),
        }
    }

    /// Builds a map of `name -> Style` for all keys present in `colors` or `styles`.
    ///
    /// Each entry is *partial*: missing fg/bg/modifiers are left unset so callers can patch it onto
    /// a base style.
    pub fn overlay_styles(&self) -> Result<HashMap<String, Style>> {
        let mut keys: BTreeSet<&str> = BTreeSet::new();
        for k in self.colors.keys() {
            keys.insert(k.as_str());
        }
        for k in self.styles.keys() {
            keys.insert(k.as_str());
        }

        let mut out = HashMap::new();
        for key in keys {
            let mut style = Style::default();

            if let Some(c) = self.colors.get(key) {
                if let Some(fg) = c.fg.as_deref() {
                    style = style.fg(parse_color(fg)
                        .with_context(|| format!("invalid fg color for key {key:?}"))?);
                }
                if let Some(bg) = c.bg.as_deref() {
                    style = style.bg(parse_color(bg)
                        .with_context(|| format!("invalid bg color for key {key:?}"))?);
                }
            }

            if let Some(mods) = self.styles.get(key) {
                let modifier = parse_modifiers(mods)
                    .with_context(|| format!("invalid modifiers for key {key:?}"))?;
                style = style.add_modifier(modifier);
            }

            out.insert(key.to_string(), style);
        }

        Ok(out)
    }
}

fn parse_color(s: &str) -> Result<Color> {
    let trimmed = s.trim();
    if let Some(hex) = trimmed.strip_prefix('#') {
        let bytes = hex.as_bytes();
        let (r, g, b) = match bytes.len() {
            3 => {
                let r = parse_short_hex_channel(bytes[0]).context("parse red channel")?;
                let g = parse_short_hex_channel(bytes[1]).context("parse green channel")?;
                let b = parse_short_hex_channel(bytes[2]).context("parse blue channel")?;
                (r, g, b)
            }
            6 => {
                let r = parse_hex_channel(bytes[0], bytes[1]).context("parse red channel")?;
                let g = parse_hex_channel(bytes[2], bytes[3]).context("parse green channel")?;
                let b = parse_hex_channel(bytes[4], bytes[5]).context("parse blue channel")?;
                (r, g, b)
            }
            _ => return Err(anyhow!("expected #RGB or #RRGGBB, got {trimmed:?}")),
        };
        return Ok(Color::Rgb(r, g, b));
    }

    let name = trimmed.to_ascii_lowercase();
    let c = match name.as_str() {
        "reset" => Color::Reset,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "darkgrey" => Color::DarkGray,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        "white" => Color::White,
        _ => return Err(anyhow!("unknown color {trimmed:?}")),
    };
    Ok(c)
}

fn parse_short_hex_channel(hex: u8) -> Result<u8> {
    let v = hex_value(hex)?;
    Ok((v << 4) | v)
}

fn parse_hex_channel(high: u8, low: u8) -> Result<u8> {
    Ok((hex_value(high)? << 4) | hex_value(low)?)
}

fn hex_value(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(anyhow!("invalid hex digit")),
    }
}

fn parse_modifiers(mods: &[String]) -> Result<Modifier> {
    let mut out = Modifier::empty();
    for m in mods {
        let name = m.trim().to_ascii_lowercase();
        let flag = match name.as_str() {
            "bold" => Modifier::BOLD,
            "dim" => Modifier::DIM,
            "italic" => Modifier::ITALIC,
            "underline" | "underlined" => Modifier::UNDERLINED,
            "reverse" | "reversed" => Modifier::REVERSED,
            "hidden" => Modifier::HIDDEN,
            "crossedout" | "crossed_out" => Modifier::CROSSED_OUT,
            "slow_blink" => Modifier::SLOW_BLINK,
            "rapid_blink" => Modifier::RAPID_BLINK,
            "" => continue,
            _ => return Err(anyhow!("unknown modifier {m:?}")),
        };
        out.insert(flag);
    }
    Ok(out)
}
