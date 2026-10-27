//! Profiles tab: one row per profile with its runner, games and prefix.

use liblootrs::{model::Severity, store};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Cell, Paragraph, Row, Table, TableState},
};

use crate::app::tui::Tui;

pub fn render(frame: &mut Frame, area: Rect, tui: &Tui) {
    let profiles_dir = store::profiles_dir(&tui.settings.dirs.config);
    let broken: Vec<_> = tui
        .catalog
        .load_errors
        .iter()
        .filter(|err| {
            err.path()
                .is_some_and(|path| path.starts_with(&profiles_dir))
        })
        .collect();
    let mut block = Block::bordered().title(" profiles ");
    if let Some(first) = broken.first() {
        let file = first
            .path()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let more = match broken.len() {
            1 => String::new(),
            n => format!(" (+{} more)", n - 1),
        };
        block =
            block.title_bottom(Line::from(format!(" ⚠ {file}: {}{more} ", first.detail())).red());
    }

    if tui.catalog.profiles.is_empty() {
        frame.render_widget(
            Paragraph::new(" no profiles — press a to add one".dim()).block(block),
            area,
        );
        return;
    }

    let rows = tui.catalog.profiles.iter().map(|profile| {
        let games = tui
            .catalog
            .games
            .iter()
            .filter(|game| game.profile == profile.name)
            .count();
        let issues = tui.profile_issues.get(&profile.name);
        let worst = issues
            .into_iter()
            .flatten()
            .min_by_key(|issue| match issue.severity {
                Severity::Error => 0,
                Severity::Warning => 1,
            });
        let (marker, note) = match worst {
            Some(issue) if issue.severity == Severity::Error => (
                Span::styled("⚠", Style::new().fg(Color::Red)),
                Span::styled(issue.message.clone(), Style::new().fg(Color::Red)),
            ),
            Some(issue) => (
                Span::styled("⚠", Style::new().fg(Color::Yellow)),
                Span::styled(issue.message.clone(), Style::new().fg(Color::Yellow)),
            ),
            None => (Span::raw(" "), Span::raw("")),
        };
        let prefix = profile
            .prefix
            .as_deref()
            .map_or("-".into(), |prefix| prefix.display().to_string());
        Row::new([
            Cell::from(marker),
            Cell::from(profile.name.clone()),
            Cell::from(profile.runner.as_str()),
            Cell::from(games.to_string()),
            Cell::from(prefix),
            Cell::from(note),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Fill(2),
            Constraint::Fill(2),
        ],
    )
    .header(Row::new(["", "NAME", "RUNNER", "GAMES", "PREFIX", "NOTES"]).bold())
    .row_highlight_style(Style::new().reversed())
    .block(block);
    let mut state = TableState::default().with_selected(Some(tui.selected()));
    frame.render_stateful_widget(table, area, &mut state);
}
