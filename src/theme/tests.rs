use ratatui::style::{Color, Modifier, Style};

use super::{Theme, ThemeConfig, ThemeConfigFormat};

struct TempThemeFile(std::path::PathBuf);

impl TempThemeFile {
    fn new(ext: &str, contents: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "atto-ui-theme-test-{}-{unique}.{ext}",
            std::process::id()
        ));
        std::fs::write(&path, contents).expect("write temp theme file");
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempThemeFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn theme_turbo_uses_classic_tv_palette_and_named_styles() {
    let theme = Theme::turbo();

    assert_eq!(theme.desktop.bg, Some(Color::Blue));
    assert_eq!(theme.menu_bar.bg, Some(Color::Cyan));
    assert_eq!(theme.status_bar.bg, Some(Color::Cyan));
    assert_eq!(theme.window_bg.bg, Some(Color::Gray));
    assert_eq!(theme.selection.bg, Some(Color::Green));
    assert_eq!(theme.menu_item_selected.bg, Some(Color::Green));
    assert_eq!(
        theme.named_style("desktop").expect("desktop token"),
        theme.desktop
    );
    assert_eq!(
        theme.named_style("selection").expect("selection token"),
        theme.selection
    );

    assert_eq!(Theme::dark().desktop.bg, Some(Color::Black));
}

#[test]
fn theme_named_resolves_turbo_aliases() {
    assert_eq!(
        Theme::named("turbo").expect("turbo").desktop.bg,
        Some(Color::Blue)
    );
    assert_eq!(
        Theme::named("Turbo Vision")
            .expect("turbo vision alias")
            .desktop
            .bg,
        Some(Color::Blue)
    );

    let err = Theme::named("missing").expect_err("unknown theme should fail");
    assert!(
        err.to_string()
            .contains("expected 'dark', 'light', or 'turbo'")
    );
}

#[test]
fn theme_default_checkbox_checked_glyph_uses_uppercase_x() {
    for theme in [Theme::dark(), Theme::light(), Theme::turbo()] {
        assert_eq!(theme.glyph("checkbox-checked"), Some("[X]"));
    }
}

#[test]
fn theme_file_can_select_turbo_base_and_overlay_it() {
    let file = TempThemeFile::new(
        "yaml",
        r##"
base: turbo
colors:
  widget-accent:
    fg: yellow
glyphs:
  close-button: "X"
"##,
    );

    let theme = Theme::load_from_path(file.path()).expect("load turbo theme file");

    assert_eq!(theme.desktop.bg, Some(Color::Blue));
    assert_eq!(theme.window_bg.bg, Some(Color::Gray));
    assert_eq!(theme.widget.accent.fg, Some(Color::Yellow));
    assert_eq!(theme.widget.accent.bg, Some(Color::Gray));
    assert_eq!(theme.glyph("close-button"), Some("X"));
}

#[test]
fn theme_file_without_base_still_uses_dark_default() {
    let file = TempThemeFile::new(
        "json",
        r##"
{
  "colors": {
    "desktop": { "fg": "#112233" }
  }
}
"##,
    );

    let theme = Theme::load_from_path(file.path()).expect("load theme file");

    assert_eq!(theme.desktop.bg, Some(Color::Black));
    assert_eq!(theme.desktop.fg, Some(Color::Rgb(0x11, 0x22, 0x33)));
}

#[test]
fn theme_file_rejects_unknown_base() {
    let file = TempThemeFile::new("yaml", "base: missing\n");

    let err = Theme::load_from_path_with_base(file.path(), Theme::light())
        .expect_err("unknown base should fail");
    let message = err
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(message.contains("invalid theme base"));
    assert!(message.contains("unknown theme"));
}

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

#[test]
fn theme_default_menu_mnemonic_uses_classic_red_accent() {
    for theme in [Theme::dark(), Theme::light(), Theme::turbo()] {
        let mnemonic = theme
            .named_style("menu-mnemonic")
            .expect("menu mnemonic token");
        assert_eq!(mnemonic.fg, Some(Color::Red));
        assert!(mnemonic.has_modifier(Modifier::UNDERLINED));
    }
}

#[test]
fn theme_config_overlays_menu_named_styles() {
    let json = r##"
    {
      "colors": {
        "menu-mnemonic": { "fg": "red" },
        "menu-item-shortcut": { "fg": "#112233" }
      },
      "styles": {
        "menu-mnemonic": ["bold"]
      }
    }
    "##;

    let cfg = ThemeConfig::from_str(json, ThemeConfigFormat::Json).expect("parse json");
    let mut theme = Theme::dark();
    theme
        .apply_config_overlay(&cfg)
        .expect("apply json overlay");

    let mnemonic = theme
        .named_style("menu-mnemonic")
        .expect("menu mnemonic token");
    assert_eq!(mnemonic.fg, Some(Color::Red));
    assert!(mnemonic.has_modifier(Modifier::BOLD));
    assert_eq!(
        theme
            .named_style("menu-item-shortcut")
            .expect("menu shortcut token")
            .fg,
        Some(Color::Rgb(0x11, 0x22, 0x33))
    );

    let yaml = r#"
colors:
  menu-border:
    fg: blue
"#;
    let cfg = ThemeConfig::from_str(yaml, ThemeConfigFormat::Yaml).expect("parse yaml");
    theme
        .apply_config_overlay(&cfg)
        .expect("apply yaml overlay");
    assert_eq!(
        theme
            .named_style("menu-border")
            .expect("menu border token")
            .fg,
        Some(Color::Blue)
    );
}

#[test]
fn theme_config_reports_malformed_json_and_yaml_errors() {
    let json_err = ThemeConfig::from_str("{", ThemeConfigFormat::Json)
        .expect_err("malformed JSON should fail");
    assert!(json_err.to_string().contains("parse theme JSON"));

    let yaml_err = ThemeConfig::from_str("colors: [", ThemeConfigFormat::Yaml)
        .expect_err("malformed YAML should fail");
    assert!(yaml_err.to_string().contains("parse theme YAML"));
}

#[test]
fn theme_config_from_bytes_infer_reports_both_format_failures() {
    let err = ThemeConfig::from_bytes_infer(b"{ not yaml: [", None)
        .expect_err("invalid inferred config should fail");
    let message = err.to_string();
    assert!(message.contains("failed to parse theme as JSON"));
    assert!(message.contains("or YAML"));
}

#[test]
fn theme_config_rejects_unknown_color_and_modifier_with_key_context() {
    let yaml = r#"
colors:
  widget-token:
    fg: ultraviolet
"#;

    let cfg = ThemeConfig::from_str(yaml, ThemeConfigFormat::Yaml).expect("parse yaml");
    let mut theme = Theme::dark();
    let err = theme
        .apply_config_overlay(&cfg)
        .expect_err("unknown color should fail first");
    let message = err
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(message.contains("invalid fg color for key \"widget-token\""));
    assert!(message.contains("unknown color"));

    let yaml = r#"
styles:
  other-token: [sparkle]
"#;
    let cfg = ThemeConfig::from_str(yaml, ThemeConfigFormat::Yaml).expect("parse yaml");
    let err = theme
        .apply_config_overlay(&cfg)
        .expect_err("unknown modifier should fail");
    let message = err
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(message.contains("invalid modifiers for key \"other-token\""));
    assert!(message.contains("unknown modifier"));
}

#[test]
fn theme_border_glyphs_fall_back_when_named_tokens_missing() {
    let mut theme = Theme::dark();
    theme.glyphs.remove("top-left-corner");
    theme.glyphs.remove("active-top-left-corner");

    assert_eq!(theme.border_set(false).top_left, "┌");
    assert_eq!(theme.border_set(true).top_left, "╔");
}
