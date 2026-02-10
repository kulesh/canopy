use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::application::state::AppState;
use crate::domain::graph::ProvenanceSource;

use super::theme::{confidence_color, kind_label, Theme};

pub(super) fn render_details(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    state: &AppState,
    theme: Theme,
    focused: bool,
) {
    let mut lines = Vec::new();

    if let Some(node) = state.selected_node() {
        lines.push(Line::from(Span::styled(
            format!(" {} ", kind_label(node.kind)),
            theme.kind_badge_style(node.kind),
        )));
        lines.push(Line::from(Span::styled(
            node.name.clone(),
            Style::default()
                .fg(theme.kind_color(node.kind))
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(vec![
            Span::styled("Path: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                node.path.display().to_string(),
                Style::default().fg(theme.text_primary),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Breadcrumb: ", Style::default().fg(theme.text_secondary)),
            Span::styled(state.breadcrumb(), Style::default().fg(theme.text_primary)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Confidence: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!("{:.2}", node.confidence),
                Style::default()
                    .fg(confidence_color(node.confidence))
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        let provenance = match node.provenance.source {
            ProvenanceSource::Ai => (" AI ", Color::Rgb(107, 203, 255)),
            ProvenanceSource::Human => (" HUMAN ", Color::Rgb(255, 191, 105)),
        };
        lines.push(Line::from(vec![
            Span::styled("Provenance: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                provenance.0,
                Style::default()
                    .fg(Color::Rgb(8, 15, 33))
                    .bg(provenance.1)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        if let Some(ts) = node.last_analyzed {
            lines.push(Line::from(vec![
                Span::styled("Last analyzed: ", Style::default().fg(theme.text_secondary)),
                Span::styled(
                    ts.format("%Y-%m-%d %H:%M:%S").to_string(),
                    Style::default().fg(theme.text_primary),
                ),
            ]));
        }

        if let Some(note) = state.recent_change_note() {
            lines.push(Line::from(Span::styled(
                note,
                Style::default().fg(theme.text_secondary),
            )));
        }
        if let Some(note) = state.blame_note() {
            lines.push(Line::from(Span::styled(
                note,
                Style::default().fg(theme.text_secondary),
            )));
        }

        if let Some(coverage) = state.coverage_for_selected() {
            lines.push(Line::from(vec![
                Span::styled("Coverage: ", Style::default().fg(theme.text_secondary)),
                Span::styled(
                    format!("{coverage:.1}%"),
                    Style::default()
                        .fg(confidence_color((coverage / 100.0).clamp(0.0, 1.0)))
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "Summary",
            Style::default()
                .fg(Color::Rgb(133, 217, 255))
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            node.summary.clone(),
            Style::default().fg(theme.text_primary),
        )));

        if !node.dependencies.is_empty() {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                "Depends on",
                Style::default()
                    .fg(Color::Rgb(255, 180, 96))
                    .add_modifier(Modifier::BOLD),
            )));
            for dep in &node.dependencies {
                if let Some(dep_node) = state.graph.node(dep) {
                    lines.push(Line::from(vec![
                        Span::styled("• ", Style::default().fg(Color::Rgb(255, 180, 96))),
                        Span::styled(
                            dep_node.name.clone(),
                            Style::default().fg(theme.text_primary),
                        ),
                    ]));
                }
            }
        }

        if !node.dependents.is_empty() {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                "Used by",
                Style::default()
                    .fg(Color::Rgb(135, 230, 170))
                    .add_modifier(Modifier::BOLD),
            )));
            for dep in &node.dependents {
                if let Some(dep_node) = state.graph.node(dep) {
                    lines.push(Line::from(vec![
                        Span::styled("• ", Style::default().fg(Color::Rgb(135, 230, 170))),
                        Span::styled(
                            dep_node.name.clone(),
                            Style::default().fg(theme.text_primary),
                        ),
                    ]));
                }
            }
        }

        if let Some(query) = &state.last_query {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                "Query",
                Style::default()
                    .fg(Color::Rgb(121, 222, 146))
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                query.query.clone(),
                Style::default().fg(Color::Rgb(166, 230, 194)),
            )));
            lines.push(Line::from(Span::styled(
                query.response.clone(),
                Style::default().fg(theme.text_primary),
            )));
        }
    }

    let title_style = if focused {
        Style::default()
            .fg(Color::Rgb(184, 227, 255))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme.panel_title)
            .add_modifier(Modifier::BOLD)
    };
    let border_style = if focused {
        Style::default().fg(Color::Rgb(126, 190, 255))
    } else {
        Style::default().fg(theme.panel_border)
    };

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .title(Span::styled(" Semantic Layer [Tab] ", title_style))
            .borders(Borders::ALL)
            .border_style(border_style),
    );
    frame.render_widget(paragraph, area);
}
