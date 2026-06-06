use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result, anyhow};
use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThemeConfig {
    #[serde(default)]
    pub glyphs: HashMap<String, String>,

    /// Named foreground/background pairs.
    #[serde(default)]
    pub colors: HashMap<String, ColorSpec>,

    /// Named modifier lists (e.g. `["bold", "reverse"]`).
    #[serde(default)]
    pub styles: HashMap<String, Vec<String>>,
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
        let (r, g, b) = match hex.len() {
            3 => {
                let r = parse_short_hex_channel(&hex[0..1]).context("parse red channel")?;
                let g = parse_short_hex_channel(&hex[1..2]).context("parse green channel")?;
                let b = parse_short_hex_channel(&hex[2..3]).context("parse blue channel")?;
                (r, g, b)
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).context("parse red channel")?;
                let g = u8::from_str_radix(&hex[2..4], 16).context("parse green channel")?;
                let b = u8::from_str_radix(&hex[4..6], 16).context("parse blue channel")?;
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

fn parse_short_hex_channel(hex: &str) -> Result<u8> {
    let v = u8::from_str_radix(hex, 16)?;
    Ok((v << 4) | v)
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
