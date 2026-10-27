//! Editor for list and `KEY=VALUE` fields, on top of the form.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, Clear, List, ListState, Paragraph},
};

use crate::app::tui::form::{Form, ListEditor, Value};

pub fn render(frame: &mut Frame, area: Rect, form: &Form, editor: &ListEditor) {
    let field = &form.fields[form.focus];
    let items = match &field.value {
        Value::List(items) | Value::Map(items) => items.as_slice(),
        _ => &[],
    };
    let area = super::popup(area, 70, 16);
    let hint = if editor.map { "KEY=VALUE" } else { "one entry" };
    let block = Block::bordered().title(format!(" {} ({hint} per line) ", field.label));
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let [list_area, input_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(2)]).areas(inner);
    if items.is_empty() {
        frame.render_widget(Line::from("(empty) press a to add").dim(), list_area);
    } else {
        let list = List::new(items.iter().map(String::as_str))
            .highlight_style(Style::new().reversed())
            .highlight_symbol("› ");
        let mut state = ListState::default().with_selected(Some(editor.selected));
        frame.render_stateful_widget(list, list_area, &mut state);
    }

    let mut lines = Vec::new();
    if let Some(input) = &editor.input {
        let mut spans = vec!["> ".bold()];
        spans.extend(super::input_spans(input));
        lines.push(Line::from(spans));
    }
    if let Some(error) = &editor.error {
        lines.push(Line::from(error.clone()).red());
    }
    frame.render_widget(Paragraph::new(lines), input_area);
}
