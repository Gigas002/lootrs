//! Terminal handling and rendering for the TUI (feature `tui`). The state and
//! everything it does live in `app::tui`; this module only draws it and turns
//! terminal events into [`Key`]s.

use std::{io, time::Duration};

use ratatui::{
    DefaultTerminal, Frame,
    crossterm::{
        event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
        execute,
        terminal::{Clear, ClearType, EnterAlternateScreen, enable_raw_mode},
    },
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Span,
};

use crate::app::tui::{Key, Tab, Tui, form::Input};

mod confirm;
mod form;
mod games_list;
mod header;
mod help;
mod list_editor;
mod log_view;
mod profiles_list;
mod prompt;
mod status_bar;

#[cfg(test)]
mod tests;

/// The terminal in raw mode on the alternate screen; restored on drop.
pub struct Terminal {
    inner: DefaultTerminal,
}

impl Terminal {
    pub fn enter() -> io::Result<Self> {
        Ok(Self {
            inner: ratatui::try_init()?,
        })
    }

    pub fn draw(&mut self, tui: &Tui) -> io::Result<()> {
        self.inner.draw(|frame| draw(frame, tui)).map(drop)
    }

    /// Waits up to `timeout` for a key press.
    pub fn poll_key(&mut self, timeout: Duration) -> io::Result<Option<Key>> {
        if !event::poll(timeout)? {
            return Ok(None);
        }
        Ok(match event::read()? {
            Event::Key(event) if event.kind == KeyEventKind::Press => key(event),
            _ => None,
        })
    }

    /// Hands the plain terminal to `f` (a foreground command), then takes it back.
    pub fn suspend<T>(&mut self, f: impl FnOnce() -> T) -> io::Result<T> {
        ratatui::restore();
        let result = f();
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, Clear(ClearType::All))?;
        // Reset both buffers so the next draw repaints everything. Not
        // `Terminal::clear`: it asks the terminal for the cursor position.
        self.inner.swap_buffers();
        self.inner.swap_buffers();
        Ok(result)
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

/// Maps a crossterm key event; `None` for keys lootrs doesn't use.
pub fn key(event: KeyEvent) -> Option<Key> {
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    Some(match event.code {
        KeyCode::Char(c) if ctrl => Key::Ctrl(c.to_ascii_lowercase()),
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        _ => return None,
    })
}

/// Draws the whole screen.
pub fn draw(frame: &mut Frame, tui: &Tui) {
    let [header_area, body, status_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(2),
    ])
    .areas(frame.area());

    header::render(frame, header_area, tui);
    let list_area = match &tui.log {
        Some(pane) => {
            let [list, log] =
                Layout::vertical([Constraint::Percentage(50), Constraint::Fill(1)]).areas(body);
            log_view::render(frame, log, pane);
            list
        }
        None => body,
    };
    match tui.tab {
        Tab::Games => games_list::render(frame, list_area, tui),
        Tab::Profiles => profiles_list::render(frame, list_area, tui),
    }
    status_bar::render(frame, status_area, tui);

    if let Some(form) = &tui.form {
        form::render(frame, body, form);
        if let Some(editor) = &form.editor {
            list_editor::render(frame, body, form, editor);
        }
    }
    match &tui.modal {
        None => {}
        Some(crate::app::tui::Modal::Help) => help::render(frame, frame.area()),
        Some(crate::app::tui::Modal::Confirm { message, .. }) => {
            confirm::render(frame, frame.area(), message);
        }
        Some(crate::app::tui::Modal::Prompt {
            title, hint, input, ..
        }) => prompt::render(frame, frame.area(), title, hint, input),
    }
}

/// A `width`×`height` rectangle centered in `area`, clamped to fit.
fn popup(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// Text with a block cursor at the input's position.
fn input_spans(input: &Input) -> Vec<Span<'static>> {
    let text = input.text();
    let at = input.cursor();
    let before: String = text.chars().take(at).collect();
    let cursor: String = text.chars().nth(at).map_or(" ".into(), String::from);
    let after: String = text.chars().skip(at + 1).collect();
    vec![
        Span::raw(before),
        Span::styled(cursor, Style::new().add_modifier(Modifier::REVERSED)),
        Span::raw(after),
    ]
}
