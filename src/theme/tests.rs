use ratatui::style::{Color, Modifier, Style};

use super::{Theme, ThemeConfig, ThemeConfigFormat};

#[test]
fn theme_config_json_overlays_preserve_unspecified_colors() {
    let json = r#"
    {
      "styles": {
        "desktop": ["reverse", "bold"]
      }
    }
    "#;

    let cfg = ThemeConfig::from_str(json, ThemeConfigFormat::Json).expect("parse json");

    let mut theme = Theme::dark();
    let before = theme.desktop;
    theme.apply_config_overlay(&cfg).expect("apply overlay");

    // No colors provided, so fg/bg should be preserved.
    assert_eq!(theme.desktop.fg, before.fg);
    assert_eq!(theme.desktop.bg, before.bg);
    assert!(theme.desktop.has_modifier(Modifier::REVERSED));
    assert!(theme.desktop.has_modifier(Modifier::BOLD));
}

#[test]
fn theme_config_yaml_overlays_colors() {
    let yaml = r##"
colors:
  desktop:
    fg: "#112233"
    bg: "#445566"
"##;

    let cfg = ThemeConfig::from_str(yaml, ThemeConfigFormat::Yaml).expect("parse yaml");

    let mut theme = Theme::dark();
    theme.apply_config_overlay(&cfg).expect("apply overlay");
    assert_eq!(theme.desktop.fg, Some(Color::Rgb(0x11, 0x22, 0x33)));
    assert_eq!(theme.desktop.bg, Some(Color::Rgb(0x44, 0x55, 0x66)));
}

#[test]
fn theme_config_accepts_short_hex_colors() {
    let yaml = r##"
colors:
  desktop:
    fg: "#fff"
    bg: "#0a7"
"##;

    let cfg = ThemeConfig::from_str(yaml, ThemeConfigFormat::Yaml).expect("parse yaml");

    let mut theme = Theme::dark();
    theme.apply_config_overlay(&cfg).expect("apply overlay");
    assert_eq!(theme.desktop.fg, Some(Color::Rgb(0xff, 0xff, 0xff)));
    assert_eq!(theme.desktop.bg, Some(Color::Rgb(0x00, 0xaa, 0x77)));
}

#[test]
fn theme_config_rejects_non_ascii_hex_without_panicking() {
    let yaml = r##"
colors:
  desktop:
    fg: "#€"
"##;

    let cfg = ThemeConfig::from_str(yaml, ThemeConfigFormat::Yaml).expect("parse yaml");

    let mut theme = Theme::dark();
    let err = theme
        .apply_config_overlay(&cfg)
        .expect_err("invalid hex should be rejected");
    assert!(err.to_string().contains("invalid fg color"));
}

#[test]
fn theme_config_preserves_custom_keys_for_user_widgets() {
    let json = r##"
    {
      "colors": { "my-widget": { "fg": "#00ff00" } },
      "styles": { "my-widget": ["bold"] }
    }
    "##;

    let cfg = ThemeConfig::from_str(json, ThemeConfigFormat::Json).expect("parse json");

    let mut theme = Theme::dark();
    theme.apply_config_overlay(&cfg).expect("apply overlay");

    let style = theme.named_style("my-widget").expect("custom style exists");
    assert_eq!(style.fg, Some(Color::Rgb(0, 255, 0)));
    assert!(style.has_modifier(Modifier::BOLD));
}

#[test]
fn theme_config_glyph_override_is_visible() {
    let json = r#"
    {
      "glyphs": { "close-button": "✕" }
    }
    "#;

    let cfg = ThemeConfig::from_str(json, ThemeConfigFormat::Json).expect("parse json");

    let mut theme = Theme::dark();
    theme.apply_config_overlay(&cfg).expect("apply overlay");

    assert_eq!(theme.glyph("close-button"), Some("✕"));
    assert_eq!(
        theme.border_set(false).top_left,
        theme
            .glyph("top-left-corner")
            .expect("default border glyph")
    );
}

#[test]
fn theme_named_style_can_be_set_programmatically() {
    let mut theme = Theme::dark();
    theme.set_named_style("my-token", Style::default().fg(Color::Red));
    assert_eq!(
        theme.named_style("my-token").expect("token exists").fg,
        Some(Color::Red)
    );
}
