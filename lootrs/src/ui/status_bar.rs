//! Bottom lines: the last message and the keys for the current screen.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
};

use crate::app::tui::{Tab, Tui};

pub fn render(frame: &mut Frame, area: Rect, tui: &Tui) {
    let [message, keys] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);

    if let Some(status) = &tui.status {
        let style = if status.error {
            Style::new().fg(Color::Red)
        } else {
            Style::new()
        };
        frame.render_widget(Line::styled(format!(" {}", status.text), style), message);
    }
    frame.render_widget(Line::from(hints(tui)).dim(), keys);
}

fn hints(tui: &Tui) -> &'static str {
    match (&tui.form, tui.tab) {
        (Some(form), _) if form.editor.is_some() => {
            " a add  ⏎ edit  d delete  J/K reorder  Esc done"
        }
        (Some(_), _) => " ↑↓ field  ←→/space change  ⏎ edit list  Ctrl-S save  Esc cancel",
        (None, Tab::Games) => {
            " ⏎ run  D debug run  s stop  a add  e edit  d delete  l log  K killall  ? help  q quit"
        }
        (None, Tab::Profiles) => {
            " i init  t tricks  x execute  a add  e edit  d delete  l log  ? help  q quit"
        }
    }
}
