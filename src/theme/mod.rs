use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Debug)]
pub struct WidgetTheme {
    pub normal: Style,
    pub focused: Style,
    pub dim: Style,
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

    pub menu_bar: Style,
    pub menu_bar_active: Style,
    pub menu_item: Style,
    pub menu_item_selected: Style,

    pub status_bar: Style,
    pub status_bar_key: Style,

    pub widget: WidgetTheme,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
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
                accent: Style::default().fg(Color::LightBlue),
            },
        }
    }

    pub fn light() -> Self {
        Self {
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
                accent: Style::default().fg(Color::Blue),
            },
        }
    }
}
