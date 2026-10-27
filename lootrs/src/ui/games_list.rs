//! Games tab: one row per game with its profile, runner and state.

use liblootrs::{model::Severity, process::Owner, store};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Cell, Row, Table, TableState},
};

use crate::app::tui::Tui;

pub fn render(frame: &mut Frame, area: Rect, tui: &Tui) {
    let games_path = store::games_path(&tui.settings.dirs.config);
    let mut block = Block::bordered().title(" games ");
    if let Some(err) = tui
        .catalog
        .load_errors
        .iter()
        .find(|err| err.path() == Some(games_path.as_path()))
    {
        block = block.title_bottom(Line::from(format!(" ⚠ games.toml: {} ", err.detail())).red());
    }

    if tui.catalog.games.is_empty() {
        frame.render_widget(
            ratatui::widgets::Paragraph::new(" no games — press a to add one".dim()).block(block),
            area,
        );
        return;
    }

    let rows = tui.catalog.games.iter().map(|game| {
        let runner = tui
            .catalog
            .profile(&game.profile)
            .map_or("?", |profile| profile.runner.as_str());
        let issues = tui.game_issues.get(&game.name);
        let error = issues
            .into_iter()
            .flatten()
            .chain(tui.profile_issues.get(&game.profile).into_iter().flatten())
            .find(|issue| issue.severity == Severity::Error);

        let (marker, state) = match (tui.running.get(&Owner::Game(game.name.clone())), error) {
            (Some(running), _) => (
                Span::styled("●", Style::new().fg(Color::Green)),
                Span::styled(
                    format!(
                        "running: {} ({} procs)",
                        running.commands.join(", "),
                        running.procs
                    ),
                    Style::new().fg(Color::Green),
                ),
            ),
            (None, Some(issue)) => (
                Span::styled("⚠", Style::new().fg(Color::Red)),
                Span::styled(issue.message.clone(), Style::new().fg(Color::Red)),
            ),
            (None, None) => (Span::raw(" "), Span::raw("")),
        };
        Row::new([
            Cell::from(marker),
            Cell::from(game.name.clone()),
            Cell::from(game.profile.clone()),
            Cell::from(runner),
            Cell::from(state),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(1),
            Constraint::Fill(2),
            Constraint::Fill(1),
            Constraint::Length(6),
            Constraint::Fill(4),
        ],
    )
    .header(Row::new(["", "NAME", "PROFILE", "RUNNER", "STATE"]).bold())
    .row_highlight_style(Style::new().reversed())
    .block(block);
    let mut state = TableState::default().with_selected(Some(tui.selected()));
    frame.render_stateful_widget(table, area, &mut state);
}
