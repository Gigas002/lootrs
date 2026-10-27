//! Yes/no dialog.

use ratatui::{
    Frame,
    layout::Rect,
    style::Stylize,
    text::Line,
    widgets::{Block, Clear, Paragraph, Wrap},
};

pub fn render(frame: &mut Frame, area: Rect, message: &str) {
    let width = u16::try_from(message.chars().count() + 6)
        .unwrap_or(u16::MAX)
        .clamp(30, 70);
    let area = super::popup(area, width, 6);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::raw(message),
            Line::raw(""),
            Line::from("y yes   n/Esc no").dim(),
        ])
        .wrap(Wrap { trim: true })
        .block(Block::bordered().title(" confirm ").bold()),
        area,
    );
}
