mod config;

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;

pub use config::{ThemeConfig, ThemeConfigFormat};

#[derive(Clone, Debug)]
pub struct WidgetTheme {
    pub normal: Style,
    pub focused: Style,
    pub dim: Style,
    pub disabled: Style,
    pub accent: Style,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub desktop: Style,
    pub desktop_dim: Style,

    pub window_border: Style,
    pub window_border_focused: Style,
    pub window_title: Style,
    pub window_title_focused: Style,
    pub window_bg: Style,
    pub window_shadow: Style,

    pub scrollbar_track: Style,
    pub scrollbar_thumb: Style,
    pub scrollbar_arrow: Style,

    pub menu_bar: Style,
    pub menu_bar_active: Style,
    pub menu_item: Style,
    pub menu_item_selected: Style,
    pub selection: Style,

    pub status_bar: Style,
    pub status_bar_key: Style,

    pub widget: WidgetTheme,

    glyphs: HashMap<String, String>,
    named_styles: HashMap<String, Style>,
    named_styles_revision: u64,
}

fn next_theme_revision() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl Theme {
    pub fn dark() -> Self {
        let mut theme = Self {
            desktop: Style::default().bg(Color::Black).fg(Color::Gray),
            desktop_dim: Style::default()
                .bg(Color::Rgb(16, 16, 16))
                .fg(Color::DarkGray),

            window_border: Style::default().fg(Color::DarkGray),
            window_border_focused: Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
            window_title: Style::default().fg(Color::Gray),
            window_title_focused: Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            window_bg: Style::default().bg(Color::Rgb(16, 16, 16)).fg(Color::Gray),
            window_shadow: Style::default().bg(Color::Rgb(8, 8, 8)),

            scrollbar_track: Style::default().fg(Color::DarkGray),
            scrollbar_thumb: Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
            scrollbar_arrow: Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),

            menu_bar: Style::default().bg(Color::Rgb(24, 24, 24)).fg(Color::Gray),
            menu_bar_active: Style::default()
                .bg(Color::Rgb(48, 48, 48))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            menu_item: Style::default().bg(Color::Rgb(24, 24, 24)).fg(Color::Gray),
            menu_item_selected: Style::default()
                .bg(Color::LightBlue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            selection: Style::default()
                .bg(Color::LightBlue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),

            status_bar: Style::default().bg(Color::Rgb(24, 24, 24)).fg(Color::Gray),
            status_bar_key: Style::default()
                .bg(Color::Rgb(24, 24, 24))
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),

            widget: WidgetTheme {
                normal: Style::default().fg(Color::Gray),
                focused: Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
                dim: Style::default().fg(Color::DarkGray),
                disabled: Style::default().fg(Color::DarkGray),
                accent: Style::default().fg(Color::LightBlue),
            },
            glyphs: default_glyphs(),
            named_styles: HashMap::new(),
            named_styles_revision: next_theme_revision(),
        };
        theme.populate_named_styles();
        theme
    }

    pub fn light() -> Self {
        let mut theme = Self {
            desktop: Style::default().bg(Color::White).fg(Color::Black),
            desktop_dim: Style::default()
                .bg(Color::Rgb(235, 235, 235))
                .fg(Color::DarkGray),

            window_border: Style::default().fg(Color::DarkGray),
            window_border_focused: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            window_title: Style::default().fg(Color::Black),
            window_title_focused: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            window_bg: Style::default()
                .bg(Color::Rgb(250, 250, 250))
                .fg(Color::Black),
            window_shadow: Style::default().bg(Color::Rgb(210, 210, 210)),

            scrollbar_track: Style::default().fg(Color::DarkGray),
            scrollbar_thumb: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            scrollbar_arrow: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),

            menu_bar: Style::default()
                .bg(Color::Rgb(240, 240, 240))
                .fg(Color::Black),
            menu_bar_active: Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            menu_item: Style::default()
                .bg(Color::Rgb(240, 240, 240))
                .fg(Color::Black),
            menu_item_selected: Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            selection: Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),

            status_bar: Style::default()
                .bg(Color::Rgb(240, 240, 240))
                .fg(Color::Black),
            status_bar_key: Style::default()
                .bg(Color::Rgb(240, 240, 240))
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),

            widget: WidgetTheme {
                normal: Style::default().fg(Color::Black),
                focused: Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
                dim: Style::default().fg(Color::DarkGray),
                disabled: Style::default().fg(Color::DarkGray),
                accent: Style::default().fg(Color::Blue),
            },
            glyphs: default_glyphs(),
            named_styles: HashMap::new(),
            named_styles_revision: next_theme_revision(),
        };
        theme.populate_named_styles();
        theme
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self> {
        Self::load_from_path_with_base(path, Theme::dark())
    }

    pub fn load_from_path_with_base(path: impl AsRef<Path>, base: Theme) -> Result<Self> {
        let path = path.as_ref();
        let bytes =
            std::fs::read(path).with_context(|| format!("read theme file {}", path.display()))?;
        let config = ThemeConfig::from_bytes_infer(&bytes, Some(path))?;

        let mut theme = base;
        theme.apply_config_overlay(&config)?;
        Ok(theme)
    }

    pub fn apply_config_overlay(&mut self, cfg: &ThemeConfig) -> Result<()> {
        for (k, v) in &cfg.glyphs {
            self.glyphs.insert(k.clone(), v.clone());
        }

        let overlays = cfg.overlay_styles()?;
        let has_style_overlay = !overlays.is_empty();
        for (k, overlay) in overlays {
            let base = self.named_styles.get(&k).copied().unwrap_or_default();
            self.named_styles.insert(k, base.patch(overlay));
        }

        self.refresh_typed_fields_from_named_styles();
        if has_style_overlay {
            self.bump_named_styles_revision();
        }
        Ok(())
    }

    pub fn glyph(&self, name: &str) -> Option<&str> {
        self.glyphs.get(name).map(String::as_str)
    }

    pub fn named_style(&self, name: &str) -> Option<Style> {
        self.named_styles.get(name).copied()
    }

    pub(crate) fn named_styles_revision(&self) -> u64 {
        self.named_styles_revision
    }

    pub fn set_glyph(&mut self, name: impl Into<String>, glyph: impl Into<String>) {
        self.glyphs.insert(name.into(), glyph.into());
    }

    pub fn set_named_style(&mut self, name: impl Into<String>, style: Style) {
        self.named_styles.insert(name.into(), style);
        self.refresh_typed_fields_from_named_styles();
        self.bump_named_styles_revision();
    }

    fn bump_named_styles_revision(&mut self) {
        self.named_styles_revision = next_theme_revision();
    }

    /// Returns a border symbol set backed by themed glyphs.
    ///
    /// When `active == true`, uses `active-*` keys; otherwise uses the normal border keys.
    pub fn border_set<'a>(&'a self, active: bool) -> border::Set<'a> {
        let (tl, tr, bl, br, h, v) = if active {
            (
                self.glyph_or("active-top-left-corner", "╔"),
                self.glyph_or("active-top-right-corner", "╗"),
                self.glyph_or("active-bottom-left-corner", "╚"),
                self.glyph_or("active-bottom-right-corner", "╝"),
                self.glyph_or("active-h-border", "═"),
                self.glyph_or("active-v-border", "║"),
            )
        } else {
            (
                self.glyph_or("top-left-corner", "┌"),
                self.glyph_or("top-right-corner", "┐"),
                self.glyph_or("bottom-left-corner", "└"),
                self.glyph_or("bottom-right-corner", "┘"),
                self.glyph_or("h-border", "─"),
                self.glyph_or("v-border", "│"),
            )
        };

        border::Set {
            top_left: tl,
            top_right: tr,
            bottom_left: bl,
            bottom_right: br,
            vertical_left: v,
            vertical_right: v,
            horizontal_top: h,
            horizontal_bottom: h,
        }
    }

    fn glyph_or<'a>(&'a self, name: &str, fallback: &'a str) -> &'a str {
        self.glyph(name).unwrap_or(fallback)
    }

    fn populate_named_styles(&mut self) {
        self.named_styles.insert("desktop".into(), self.desktop);
        self.named_styles
            .insert("desktop-dim".into(), self.desktop_dim);

        self.named_styles
            .insert("inactive-window-border".into(), self.window_border);
        self.named_styles
            .insert("active-window-border".into(), self.window_border_focused);
        self.named_styles
            .insert("inactive-window-title".into(), self.window_title);
        self.named_styles
            .insert("active-window-title".into(), self.window_title_focused);
        self.named_styles
            .insert("tab-title-inactive".into(), self.window_title);
        self.named_styles
            .insert("tab-title-active".into(), self.window_title_focused);
        self.named_styles
            .insert("tab-title-separator".into(), self.window_title);
        self.named_styles
            .insert("tab-title-marker".into(), self.window_title_focused);
        self.named_styles.insert("window-bg".into(), self.window_bg);
        self.named_styles
            .insert("window-shadow".into(), self.window_shadow);
        self.named_styles.insert(
            "drag-ghost".into(),
            self.window_bg
                .patch(self.widget.accent.add_modifier(Modifier::BOLD)),
        );
        self.named_styles.insert(
            "drop-target-active".into(),
            Style::default().bg(Color::Rgb(24, 64, 48)).fg(Color::White),
        );
        self.named_styles.insert(
            "drop-target-reject".into(),
            Style::default().bg(Color::Rgb(96, 32, 32)).fg(Color::White),
        );
        self.named_styles.insert(
            "drop-insertion-marker".into(),
            self.widget.accent.add_modifier(Modifier::BOLD),
        );

        self.named_styles
            .insert("scrollbar-track".into(), self.scrollbar_track);
        self.named_styles
            .insert("scrollbar-thumb".into(), self.scrollbar_thumb);
        self.named_styles
            .insert("scrollbar-arrow".into(), self.scrollbar_arrow);

        self.named_styles.insert("menu-bar".into(), self.menu_bar);
        self.named_styles
            .insert("menu-bar-active".into(), self.menu_bar_active);
        self.named_styles.insert("menu-item".into(), self.menu_item);
        self.named_styles
            .insert("menu-item-selected".into(), self.menu_item_selected);
        self.named_styles.insert("selection".into(), self.selection);

        self.named_styles
            .insert("status-bar".into(), self.status_bar);
        self.named_styles
            .insert("status-bar-key".into(), self.status_bar_key);

        self.named_styles
            .insert("widget-normal".into(), self.widget.normal);
        self.named_styles
            .insert("widget-focused".into(), self.widget.focused);
        self.named_styles
            .insert("widget-dim".into(), self.widget.dim);
        self.named_styles
            .insert("widget-disabled".into(), self.widget.disabled);
        self.named_styles
            .insert("widget-accent".into(), self.widget.accent);

        self.named_styles
            .insert("tab-active".into(), self.widget.focused);
        self.named_styles
            .insert("tab-inactive".into(), self.widget.normal);
        self.named_styles
            .insert("tab-separator".into(), self.widget.dim);
        self.named_styles
            .insert("tab-header".into(), self.widget.normal);

        self.named_styles
            .insert("disclosure-title".into(), self.widget.normal);
        self.named_styles
            .insert("disclosure-title-focused".into(), self.widget.focused);
        self.named_styles
            .insert("disclosure-idle".into(), self.widget.dim);
        self.named_styles.insert(
            "disclosure-running".into(),
            self.widget.accent.add_modifier(Modifier::BOLD),
        );
        self.named_styles
            .insert("disclosure-done".into(), self.widget.dim);
        self.named_styles.insert(
            "disclosure-error".into(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        );
        self.named_styles.insert(
            "disclosure-content".into(),
            self.window_bg.patch(self.widget.normal),
        );
        self.named_styles
            .insert("windowed-text-footer".into(), self.widget.dim);
        self.named_styles.insert(
            "toast-info".into(),
            self.window_bg.patch(self.widget.accent),
        );
        self.named_styles.insert(
            "toast-success".into(),
            self.window_bg.patch(
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        );
        self.named_styles.insert(
            "toast-warning".into(),
            self.window_bg.patch(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        );
        self.named_styles.insert(
            "toast-error".into(),
            self.window_bg
                .patch(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        );
        self.named_styles
            .insert("image-fallback".into(), self.widget.dim);

        let markdown_base = self.window_bg.patch(self.widget.normal);
        self.named_styles
            .insert("markdown-base".into(), markdown_base);
        self.named_styles.insert(
            "markdown-heading-1".into(),
            self.widget
                .focused
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        );
        self.named_styles.insert(
            "markdown-heading-2".into(),
            self.widget
                .normal
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        );
        self.named_styles.insert(
            "markdown-heading-3".into(),
            self.widget.normal.add_modifier(Modifier::BOLD),
        );
        self.named_styles.insert(
            "markdown-heading-4".into(),
            self.widget.normal.add_modifier(Modifier::BOLD),
        );
        self.named_styles
            .insert("markdown-heading-5".into(), self.widget.normal);
        self.named_styles
            .insert("markdown-heading-6".into(), self.widget.dim);
        self.named_styles.insert(
            "markdown-bold".into(),
            Style::default().add_modifier(Modifier::BOLD),
        );
        self.named_styles.insert(
            "markdown-italic".into(),
            Style::default().add_modifier(Modifier::ITALIC),
        );
        self.named_styles.insert(
            "markdown-strikethrough".into(),
            Style::default().add_modifier(Modifier::CROSSED_OUT),
        );
        self.named_styles
            .insert("markdown-blockquote".into(), self.widget.dim);
        self.named_styles
            .insert("markdown-list-bullet".into(), self.widget.accent);
        self.named_styles
            .insert("markdown-code-inline".into(), self.widget.accent);
        self.named_styles
            .insert("markdown-code-block".into(), markdown_base);
        self.named_styles
            .insert("markdown-table-border".into(), self.widget.dim);
        self.named_styles.insert(
            "markdown-table-header".into(),
            self.widget.accent.add_modifier(Modifier::BOLD),
        );
        self.named_styles
            .insert("markdown-table-cell".into(), markdown_base);
        self.named_styles.insert(
            "markdown-link".into(),
            self.widget.accent.add_modifier(Modifier::UNDERLINED),
        );
        self.named_styles
            .insert("markdown-mark".into(), self.widget.dim);
    }

    fn refresh_typed_fields_from_named_styles(&mut self) {
        if let Some(v) = self.named_styles.get("desktop") {
            self.desktop = *v;
        }
        if let Some(v) = self.named_styles.get("desktop-dim") {
            self.desktop_dim = *v;
        }

        if let Some(v) = self.named_styles.get("inactive-window-border") {
            self.window_border = *v;
        }
        if let Some(v) = self.named_styles.get("active-window-border") {
            self.window_border_focused = *v;
        }
        if let Some(v) = self.named_styles.get("inactive-window-title") {
            self.window_title = *v;
        }
        if let Some(v) = self.named_styles.get("active-window-title") {
            self.window_title_focused = *v;
        }
        if let Some(v) = self.named_styles.get("window-bg") {
            self.window_bg = *v;
        }
        if let Some(v) = self.named_styles.get("window-shadow") {
            self.window_shadow = *v;
        }

        if let Some(v) = self.named_styles.get("scrollbar-track") {
            self.scrollbar_track = *v;
        }
        if let Some(v) = self.named_styles.get("scrollbar-thumb") {
            self.scrollbar_thumb = *v;
        }
        if let Some(v) = self.named_styles.get("scrollbar-arrow") {
            self.scrollbar_arrow = *v;
        }

        if let Some(v) = self.named_styles.get("menu-bar") {
            self.menu_bar = *v;
        }
        if let Some(v) = self.named_styles.get("menu-bar-active") {
            self.menu_bar_active = *v;
        }
        if let Some(v) = self.named_styles.get("menu-item") {
            self.menu_item = *v;
        }
        if let Some(v) = self.named_styles.get("menu-item-selected") {
            self.menu_item_selected = *v;
        }
        if let Some(v) = self.named_styles.get("selection") {
            self.selection = *v;
        }

        if let Some(v) = self.named_styles.get("status-bar") {
            self.status_bar = *v;
        }
        if let Some(v) = self.named_styles.get("status-bar-key") {
            self.status_bar_key = *v;
        }

        if let Some(v) = self.named_styles.get("widget-normal") {
            self.widget.normal = *v;
        }
        if let Some(v) = self.named_styles.get("widget-focused") {
            self.widget.focused = *v;
        }
        if let Some(v) = self.named_styles.get("widget-dim") {
            self.widget.dim = *v;
        }
        if let Some(v) = self.named_styles.get("widget-disabled") {
            self.widget.disabled = *v;
        }
        if let Some(v) = self.named_styles.get("widget-accent") {
            self.widget.accent = *v;
        }
    }
}

fn default_glyphs() -> HashMap<String, String> {
    let mut g = HashMap::new();

    // Standard border set.
    g.insert("h-border".into(), "─".into());
    g.insert("v-border".into(), "│".into());
    g.insert("top-left-corner".into(), "┌".into());
    g.insert("top-right-corner".into(), "┐".into());
    g.insert("bottom-left-corner".into(), "└".into());
    g.insert("bottom-right-corner".into(), "┘".into());

    // Active border set (matches current focused window visuals).
    g.insert("active-h-border".into(), "═".into());
    g.insert("active-v-border".into(), "║".into());
    g.insert("active-top-left-corner".into(), "╔".into());
    g.insert("active-top-right-corner".into(), "╗".into());
    g.insert("active-bottom-left-corner".into(), "╚".into());
    g.insert("active-bottom-right-corner".into(), "╝".into());

    // Window titlebar buttons.
    g.insert("minimize-button".into(), "−".into());
    g.insert("maximize-button".into(), "□".into());
    g.insert("close-button".into(), "×".into());

    // Tab window titlebar glyphs.
    g.insert("tab-separator".into(), "|".into());
    g.insert("tab-active-left".into(), ">".into());
    g.insert("tab-active-right".into(), "<".into());

    // Controls.
    //
    // Defaults match current UI output (ASCII bracket/paren style) so existing snapshots and PTY
    // tests remain stable. Themes may override these with single-glyph variants like "☐"/"☑" or
    // "◯"/"◉".
    g.insert("checkbox-unchecked".into(), "[ ]".into());
    g.insert("checkbox-checked".into(), "[x]".into());
    g.insert("radio-unselected".into(), "( )".into());
    g.insert("radio-selected".into(), "(*)".into());
    g.insert("disclosure-collapsed".into(), ">".into());
    g.insert("disclosure-expanded".into(), "v".into());
    g.insert("disclosure-idle-indicator".into(), "[ ]".into());
    g.insert("disclosure-running-indicator".into(), "[~]".into());
    g.insert("disclosure-done-indicator".into(), "[x]".into());
    g.insert("disclosure-error-indicator".into(), "[!]".into());

    // Scrollbars (default matches current behavior).
    g.insert("scrollbar-track".into(), "░".into());
    g.insert("scrollbar-thumb".into(), "█".into());
    g.insert("scrollbar-up-arrow".into(), "▲".into());
    g.insert("scrollbar-down-arrow".into(), "▼".into());
    g.insert("scrollbar-left-arrow".into(), "◄".into());
    g.insert("scrollbar-right-arrow".into(), "►".into());

    // Tabs.
    g.insert("tab-separator".into(), "|".into());
    g.insert("tab-active-left".into(), ">".into());
    g.insert("tab-active-right".into(), "<".into());

    g
}

#[cfg(test)]
mod tests;
