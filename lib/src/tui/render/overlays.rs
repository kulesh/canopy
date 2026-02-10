use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::application::state::{AppMode, AppState};

use super::theme::Theme;

pub(super) fn render_status(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    state: &AppState,
    theme: Theme,
) {
    let mut lines = vec![Line::from(vec![
        Span::styled(
            " STATUS ",
            Style::default()
                .fg(Color::Rgb(6, 12, 26))
                .bg(theme.status_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            state.status_line.clone(),
            Style::default().fg(theme.text_primary),
        ),
    ])];
    let mode_note = match state.mode {
        AppMode::Normal => "NORMAL",
        AppMode::Project => "PROJECT",
        AppMode::Search => "SEARCH",
        AppMode::Query => "QUERY",
        AppMode::EditSummary => "EDIT SUMMARY",
        AppMode::EditReason => "EDIT REASON",
        AppMode::Help => "HELP",
        AppMode::History => "HISTORY",
        AppMode::ConfirmRegenerate => "CONFIRM REGENERATE",
        AppMode::ConfirmExport => "CONFIRM EXPORT",
    };
    lines.push(Line::from(vec![
        Span::styled(
            format!(" MODE: {mode_note} "),
            Style::default()
                .fg(Color::Rgb(13, 20, 39))
                .bg(Color::Rgb(135, 230, 170))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            "q quit | Esc up/cancel | hjkl move | Tab cycle panes | Shift-Tab reverse",
            Style::default().fg(theme.text_secondary),
        ),
    ]));

    if matches!(
        state.mode,
        AppMode::Search | AppMode::Query | AppMode::EditSummary | AppMode::EditReason
    ) {
        let label = match state.mode {
            AppMode::Search => "Search",
            AppMode::Query => "Query",
            AppMode::EditSummary => "Edit",
            AppMode::EditReason => "Reason",
            _ => "Input",
        };
        let value = if matches!(state.mode, AppMode::EditSummary) {
            state.edit_buffer.as_str()
        } else if matches!(state.mode, AppMode::EditReason) {
            state.reason_buffer.as_str()
        } else {
            state.input_buffer.as_str()
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{label}: "),
                Style::default()
                    .fg(Color::Rgb(13, 20, 39))
                    .bg(Color::Rgb(255, 184, 112))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(value.to_string(), Style::default().fg(theme.text_primary)),
        ]));
    }

    lines.push(Line::from(Span::styled(
        "?: help | P project view | / search | : query (Up/Down history) | e edit | r regenerate | v graph | i impact | H history | X export | PgUp/PgDn scroll right pane",
        Style::default().fg(theme.text_secondary),
    )));

    let widget = Paragraph::new(lines)
        .style(Style::default().bg(theme.status_bg))
        .block(
            Block::default()
                .title(Span::styled(
                    " Key Bindings ",
                    Style::default()
                        .fg(theme.panel_title)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.panel_border)),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, area);
}

pub(super) fn render_help_modal(frame: &mut ratatui::Frame<'_>, theme: Theme) {
    let area = centered_rect(80, 72, frame.area());
    frame.render_widget(Clear, area);

    let lines = vec![
        Line::from(Span::styled(
            "Navigation",
            Style::default()
                .fg(Color::Rgb(133, 217, 255))
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("Tab / Shift-Tab  : Cycle panel focus"),
        Line::from("j k / Up Down     : Move tree or scroll right pane"),
        Line::from("h l / Left Right  : Up/down tree or horizontal intent"),
        Line::from("Enter             : Drill into selected node"),
        Line::from("Backspace         : Navigate up in tree"),
        Line::from("Esc               : Navigate up (normal mode) / close modal"),
        Line::from("gg / G            : Jump top / bottom"),
        Line::from("PgUp / PgDn       : Scroll right panel quickly"),
        Line::default(),
        Line::from(Span::styled(
            "Architecture & Analysis",
            Style::default()
                .fg(Color::Rgb(133, 217, 255))
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("v                 : Toggle dependency graph"),
        Line::from("i                 : Toggle impact analysis"),
        Line::from("H                 : Toggle edit history"),
        Line::from("l                 : Collapse/expand selected node"),
        Line::default(),
        Line::from(Span::styled(
            "Query & Editing",
            Style::default()
                .fg(Color::Rgb(133, 217, 255))
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("/                 : Search"),
        Line::from(":                 : Natural language query"),
        Line::from("Up/Down (query)   : Query history recall"),
        Line::from("e                 : Edit summary"),
        Line::from("r                 : Regenerate summary"),
        Line::from("X                 : Export edit log"),
        Line::from("P                 : Toggle project view"),
        Line::from("o c R s           : Queue/cancel/retry/switch (project view)"),
        Line::default(),
        Line::from(Span::styled(
            "Press ? or Esc to close",
            Style::default()
                .fg(theme.text_secondary)
                .add_modifier(Modifier::ITALIC),
        )),
    ];

    let popup = Paragraph::new(lines).block(
        Block::default()
            .title(Span::styled(
                " Help ",
                Style::default()
                    .fg(theme.panel_title)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(126, 190, 255))),
    );
    frame.render_widget(popup, area);
}

fn centered_rect(
    percent_x: u16,
    percent_y: u16,
    r: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}
