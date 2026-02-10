use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use crate::application::state::AppState;
use crate::domain::OnboardingPhase;

use super::theme::Theme;

pub(super) fn render_project_panel(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    state: &AppState,
    theme: Theme,
) {
    let Some(project) = state.project.as_ref() else {
        let empty = Paragraph::new("No project file loaded")
            .style(Style::default().fg(theme.text_primary))
            .block(
                Block::default()
                    .title(Span::styled(
                        " Project View ",
                        Style::default()
                            .fg(theme.panel_title)
                            .add_modifier(Modifier::BOLD),
                    ))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.panel_border)),
            );
        frame.render_widget(empty, area);
        return;
    };

    let mut lines = vec![Line::from(vec![
        Span::styled(
            " Manifest ",
            Style::default()
                .fg(Color::Rgb(6, 12, 26))
                .bg(Color::Rgb(133, 217, 255))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            project.manifest_path.display().to_string(),
            Style::default().fg(theme.text_primary),
        ),
    ])];
    lines.push(Line::from(vec![
        Span::styled(
            " Active ",
            Style::default()
                .fg(Color::Rgb(6, 12, 26))
                .bg(Color::Rgb(135, 230, 170))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            project
                .active_repository_id
                .clone()
                .unwrap_or_else(|| "none".to_string()),
            Style::default().fg(theme.text_primary),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        "Up/Down select | o queue | c cancel | R retry | s/Enter switch | Esc close",
        Style::default().fg(theme.text_secondary),
    )));

    let header_height = lines.len() as u16 + 2;
    let header_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: header_height.min(area.height),
    };
    let list_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y + header_area.height,
        width: area.width,
        height: area.height.saturating_sub(header_area.height),
    };

    let header = Paragraph::new(lines).wrap(Wrap { trim: true }).block(
        Block::default()
            .title(Span::styled(
                " Project View ",
                Style::default()
                    .fg(theme.panel_title)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(126, 190, 255))),
    );
    frame.render_widget(header, header_area);

    let items: Vec<ListItem<'_>> = project
        .project
        .repositories
        .iter()
        .enumerate()
        .map(|(index, repository)| {
            let state_row = project.runtime_state.repositories.get(&repository.id);
            let phase = state_row
                .map(|runtime| phase_label(runtime.phase))
                .unwrap_or("not_started");
            let progress = state_row
                .and_then(|runtime| runtime.progress_percent)
                .map(|percent| format!("{percent:>3}%"))
                .unwrap_or_else(|| " --%".to_string());
            let message = state_row
                .and_then(|runtime| runtime.message.as_ref())
                .map(ToString::to_string)
                .unwrap_or_default();
            let last_error = state_row
                .and_then(|runtime| runtime.last_error.as_ref())
                .map(ToString::to_string)
                .unwrap_or_default();
            let selected = index == project.selected_repository_index;
            let active = project
                .active_repository_id
                .as_ref()
                .map(|id| id == &repository.id)
                .unwrap_or(false);

            let mut spans = vec![
                Span::styled(
                    if selected { "▌ " } else { "  " },
                    Style::default().fg(theme.text_secondary),
                ),
                Span::styled(
                    if active { "● " } else { "○ " },
                    Style::default().fg(if active {
                        Color::Rgb(121, 222, 146)
                    } else {
                        theme.text_secondary
                    }),
                ),
                Span::styled(
                    format!("{: <16}", repository.name),
                    Style::default().fg(theme.text_primary),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("{phase: <12}"),
                    Style::default().fg(phase_color(phase)),
                ),
                Span::raw(" "),
                Span::styled(progress, Style::default().fg(theme.text_secondary)),
            ];

            if !message.is_empty() {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    message,
                    Style::default().fg(theme.text_primary),
                ));
            }
            if !last_error.is_empty() {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!("error: {last_error}"),
                    Style::default().fg(Color::Rgb(255, 140, 120)),
                ));
            }

            let mut style = Style::default();
            if index % 2 == 1 {
                style = style.bg(theme.row_alt_bg);
            }
            if selected {
                style = Style::default()
                    .bg(theme.row_selected_bg)
                    .fg(theme.row_selected_fg);
            }
            ListItem::new(Line::from(spans)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(Span::styled(
                " Repositories ",
                Style::default()
                    .fg(theme.panel_title)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.panel_border)),
    );
    frame.render_widget(list, list_area);
}

fn phase_label(phase: OnboardingPhase) -> &'static str {
    match phase {
        OnboardingPhase::NotStarted => "not_started",
        OnboardingPhase::Queued => "queued",
        OnboardingPhase::Discovering => "discovering",
        OnboardingPhase::Policy => "policy",
        OnboardingPhase::Mapping => "mapping",
        OnboardingPhase::Validating => "validating",
        OnboardingPhase::Ready => "ready",
        OnboardingPhase::Failed => "failed",
        OnboardingPhase::Canceled => "canceled",
    }
}

fn phase_color(phase: &str) -> Color {
    match phase {
        "ready" => Color::Rgb(121, 222, 146),
        "failed" => Color::Rgb(255, 140, 120),
        "canceled" => Color::Rgb(255, 196, 125),
        "mapping" | "discovering" | "policy" | "validating" => Color::Rgb(133, 217, 255),
        _ => Color::Rgb(176, 189, 210),
    }
}
