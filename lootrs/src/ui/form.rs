//! Game and profile forms. Fields the selected runner ignores are dimmed, not hidden.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};

use crate::app::tui::form::{Field, Form, Value};

const LABEL_WIDTH: usize = 20;

pub fn render(frame: &mut Frame, area: Rect, form: &Form) {
    let runner = form.runner();
    let mut lines: Vec<Line> = Vec::new();
    for (index, field) in form.fields.iter().enumerate() {
        let focused = index == form.focus;
        let applies = form.applies(field);
        let mut spans = vec![Span::styled(
            format!(
                "{} {:<LABEL_WIDTH$}",
                if focused { "›" } else { " " },
                field.label
            ),
            if focused {
                Style::new().bold()
            } else {
                Style::new()
            },
        )];
        spans.extend(value_spans(field, focused));
        if field.read_only {
            spans.push(" (can't be renamed)".dim());
        }
        if !applies && let Some(runner) = runner {
            spans.push(format!("  (ignored for {runner})").dim());
        }
        let mut line = Line::from(spans);
        if !applies {
            line = line.patch_style(Style::new().add_modifier(Modifier::DIM));
        }
        lines.push(line);
    }

    if !form.errors.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from("not saved:").red().bold());
        for error in &form.errors {
            lines.push(Line::from(format!("  {error}")).red());
        }
    }

    // Keep the focused field visible.
    let visible = area.height.saturating_sub(2);
    let focus = u16::try_from(form.focus).unwrap_or(u16::MAX);
    let scroll = (focus + 1).saturating_sub(visible);

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).scroll((scroll, 0)).block(
            Block::bordered()
                .title(format!(" {} ", form.title()))
                .bold(),
        ),
        area,
    );
}

fn value_spans(field: &Field, focused: bool) -> Vec<Span<'static>> {
    match &field.value {
        Value::Text(input) if focused && !field.read_only => super::input_spans(input),
        Value::Text(input) if input.text().is_empty() => {
            vec![Span::styled(
                field.placeholder.clone(),
                Style::new().dim().italic(),
            )]
        }
        Value::Text(input) => vec![Span::raw(input.text().to_owned())],
        Value::Choice { options, selected } => {
            let value = options.get(*selected).cloned().unwrap_or_default();
            if focused {
                vec![Span::styled(
                    format!("‹ {value} ›"),
                    Style::new().fg(Color::Cyan),
                )]
            } else {
                vec![Span::raw(value)]
            }
        }
        Value::Toggle(on) => vec![Span::raw(if *on { "[x]" } else { "[ ]" })],
        Value::List(items) | Value::Map(items) => {
            let mut spans = if items.is_empty() {
                vec![Span::styled("(none)", Style::new().dim())]
            } else {
                vec![Span::raw(items.join("  "))]
            };
            if focused {
                spans.push(Span::styled("  ⏎ edit", Style::new().fg(Color::Cyan)));
            }
            spans
        }
    }
}
