use std::fs;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::application::state::{node_code_path, render_edit_history, AppMode, AppState};

use super::syntax::highlight_code_lines;
use super::theme::Theme;

pub(super) fn render_right_panel(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    state: &AppState,
    theme: Theme,
    focused: bool,
) {
    let title = if state.show_dependency_graph {
        "Dependency Graph"
    } else if state.show_impact {
        "Impact Analysis"
    } else if state.mode == AppMode::History {
        "Edit History"
    } else {
        "Code"
    };

    let text = if state.show_dependency_graph {
        state.dependency_graph_ascii()
    } else if state.show_impact {
        state.impact_report()
    } else if state.mode == AppMode::History {
        let selected = state.selected_node_id();
        match state.persistence.read_edits() {
            Ok(edits) => {
                let rendered = render_edit_history(&edits, selected);
                if rendered.is_empty() {
                    "No history for selection".to_string()
                } else {
                    let mut lines = Vec::new();
                    for (node, entries) in rendered {
                        lines.push(format!("{node}:"));
                        for line in entries.iter().take(8) {
                            lines.push(format!("  {line}"));
                        }
                    }
                    lines.join("\n")
                }
            }
            Err(err) => format!("Unable to read history: {err}"),
        }
    } else if let Some(node_id) = state.selected_node_id() {
        if let Some(code_path) = node_code_path(&state.repository.root, &state.graph, node_id) {
            match fs::read_to_string(&code_path) {
                Ok(contents) => contents,
                Err(_) => format!("Cannot read {}", code_path.display()),
            }
        } else {
            "No code view for this node".to_string()
        }
    } else {
        String::new()
    };

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

    let paragraph = if title == "Code" {
        Paragraph::new(highlight_code_lines(&text, theme))
            .scroll((state.right_scroll.min(u16::MAX as usize) as u16, 0))
            .block(
                Block::default()
                    .title(Span::styled(
                        format!(" {title} [Tab, j/k, PgUp/PgDn] "),
                        title_style,
                    ))
                    .borders(Borders::ALL)
                    .border_style(border_style),
            )
            .wrap(Wrap { trim: false })
    } else {
        Paragraph::new(text)
            .style(Style::default().fg(theme.text_primary))
            .scroll((state.right_scroll.min(u16::MAX as usize) as u16, 0))
            .block(
                Block::default()
                    .title(Span::styled(
                        format!(" {title} [Tab, j/k, PgUp/PgDn] "),
                        title_style,
                    ))
                    .borders(Borders::ALL)
                    .border_style(border_style),
            )
            .wrap(Wrap { trim: false })
    };
    frame.render_widget(paragraph, area);
}
