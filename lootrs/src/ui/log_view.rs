//! Tail of the selected game's (or profile's) latest run log.

use ratatui::{
    Frame,
    layout::Rect,
    style::Stylize,
    text::Line,
    widgets::{Block, Paragraph},
};

use crate::app::tui::LogPane;

pub fn render(frame: &mut Frame, area: Rect, pane: &LogPane) {
    let title = match &pane.path {
        Some(path) => format!(" log: {} (L opens it) ", path.display()),
        None => " log ".to_owned(),
    };
    let block = Block::bordered().title(title);
    if pane.path.is_none() {
        frame.render_widget(Paragraph::new(" no logs yet".dim()).block(block), area);
        return;
    }
    let visible = usize::from(area.height.saturating_sub(2));
    let skip = pane.lines.len().saturating_sub(visible);
    let lines: Vec<Line> = pane.lines[skip..]
        .iter()
        .map(|line| Line::raw(line.as_str()))
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
