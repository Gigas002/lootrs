//! Key reference.

use ratatui::{
    Frame,
    layout::Rect,
    style::Stylize,
    text::Line,
    widgets::{Block, Clear, Paragraph},
};

const KEYS: &[(&str, &str)] = &[
    ("Tab, 1, 2", "switch between games and profiles"),
    ("↑↓ j k, g G", "move"),
    ("l / L", "toggle the log pane / open the log in $PAGER"),
    ("a e d", "add / edit / delete"),
    ("R", "reload games and profiles from disk"),
    ("K", "kill everything lootrs started"),
    ("q, Ctrl-C", "quit (running games keep running)"),
    ("", ""),
    ("Games", ""),
    ("⏎ / D", "run / run with debug logging"),
    ("s", "stop"),
    ("", ""),
    ("Profiles", ""),
    ("i", "create or update the prefix"),
    ("t", "winetricks"),
    ("x", "execute a program in the prefix"),
    ("", ""),
    ("Forms", ""),
    ("↑↓ Tab", "move between fields"),
    ("←→ space", "change a choice or toggle"),
    ("⏎", "edit a list (a add, d delete, J/K reorder)"),
    ("Ctrl-S / Esc", "save / cancel"),
];

pub fn render(frame: &mut Frame, area: Rect) {
    let height = u16::try_from(KEYS.len() + 4).unwrap_or(u16::MAX);
    let area = super::popup(area, 64, height);
    let mut lines: Vec<Line> = KEYS
        .iter()
        .map(|(keys, what)| match what.is_empty() {
            true => Line::from(*keys).bold(),
            false => Line::from(format!("  {keys:<14} {what}")),
        })
        .collect();
    lines.push(Line::raw(""));
    lines.push(Line::from("any key closes; files can also be edited by hand").dim());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" help ")),
        area,
    );
}
