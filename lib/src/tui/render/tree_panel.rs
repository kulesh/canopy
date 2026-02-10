use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::application::state::AppState;
use crate::domain::NodeKind;

use super::theme::{kind_label, Theme};

pub(super) fn render_tree(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    state: &AppState,
    theme: Theme,
    focused: bool,
) {
    let items: Vec<ListItem<'_>> = state
        .visible
        .iter()
        .enumerate()
        .map(|(index, (id, depth))| {
            let node = state.graph.node(id);
            let name = node.map(|n| n.name.clone()).unwrap_or_else(|| id.clone());
            let kind = node.map(|n| n.kind).unwrap_or(NodeKind::Component);
            let kind_label = kind_label(kind);
            let prefix = "  ".repeat(*depth);
            let selected = index == state.selected_index;
            let marker = if selected { "▌" } else { " " };
            let has_children = node.map(|n| !n.children.is_empty()).unwrap_or(false);
            let expanded_marker = if has_children {
                if state.collapsed.contains(id) {
                    "▸"
                } else {
                    "▾"
                }
            } else {
                " "
            };
            let mut line = Line::from(vec![
                Span::styled(
                    format!("{marker}{prefix}{expanded_marker} "),
                    Style::default().fg(if selected {
                        theme.row_selected_fg
                    } else {
                        theme.text_secondary
                    }),
                ),
                Span::styled(format!(" {} ", kind_label), theme.kind_badge_style(kind)),
                Span::raw(" "),
                Span::styled(
                    name,
                    Style::default()
                        .fg(if selected {
                            theme.row_selected_fg
                        } else {
                            theme.kind_color(kind)
                        })
                        .add_modifier(Modifier::BOLD),
                ),
            ]);
            if selected {
                line.spans.push(Span::styled(
                    "  <",
                    Style::default().fg(Color::Rgb(255, 220, 136)),
                ));
            }

            let mut item_style = Style::default();
            if index % 2 == 1 {
                item_style = item_style.bg(theme.row_alt_bg);
            }
            if selected {
                item_style = Style::default()
                    .bg(theme.row_selected_bg)
                    .fg(theme.row_selected_fg);
            }

            ListItem::new(line).style(item_style)
        })
        .collect();

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

    let list = List::new(items)
        .block(
            Block::default()
                .title(Span::styled(" Architecture [Tab] ", title_style))
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(Style::default().bg(theme.row_selected_bg));
    frame.render_widget(list, area);
}
