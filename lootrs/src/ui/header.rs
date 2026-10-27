//! Top line: app name, tabs, and what is running.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::Tabs,
};

use crate::app::tui::Tui;

pub fn render(frame: &mut Frame, area: Rect, tui: &Tui) {
    let [name, tabs, activity] = Layout::horizontal([
        Constraint::Length(8),
        Constraint::Fill(1),
        Constraint::Length(30),
    ])
    .areas(area);

    frame.render_widget(Line::from(" lootrs ").bold().reversed(), name);
    frame.render_widget(
        Tabs::new(["1 Games", "2 Profiles"])
            .select(tui.tab as usize)
            .highlight_style(Style::new().bold().underlined())
            .divider("│"),
        tabs,
    );

    let mut parts = Vec::new();
    if !tui.running.is_empty() {
        parts.push(format!("● {} running", tui.running.len()));
    }
    if tui.jobs > 0 {
        parts.push("working…".to_owned());
    }
    frame.render_widget(
        Line::from(parts.join("  "))
            .style(Style::new().fg(Color::Green))
            .right_aligned(),
        activity,
    );
}
