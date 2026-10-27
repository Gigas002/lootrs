//! One-line text prompt (tricks verbs, execute path).

use ratatui::{
    Frame,
    layout::Rect,
    style::Stylize,
    text::Line,
    widgets::{Block, Clear, Paragraph},
};

use crate::app::tui::form::Input;

pub fn render(frame: &mut Frame, area: Rect, title: &str, hint: &str, input: &Input) {
    let area = super::popup(area, 76, 6);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(hint.to_owned()).dim(),
            Line::from(super::input_spans(input)),
            Line::raw(""),
            Line::from("⏎ ok   Esc cancel").dim(),
        ])
        .block(Block::bordered().title(format!(" {title} "))),
        area,
    );
}
