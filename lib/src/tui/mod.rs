use std::fs;
use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Terminal;

use crate::application::state::{
    map_key_to_action, node_code_path, render_edit_history, AppMode, AppState, FocusPane, KeyAction,
};
use crate::domain::graph::ProvenanceSource;
use crate::domain::NodeKind;
use crate::error::Result;

pub fn run_tui(state: &mut AppState) -> Result<()> {
    enable_raw_mode().map_err(|source| crate::error::CanopyError::io("terminal", source))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|source| crate::error::CanopyError::io("terminal", source))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)
        .map_err(|source| crate::error::CanopyError::io("terminal", source))?;

    let result = loop {
        state.poll_inference_events();
        terminal
            .draw(|frame| render(frame, state))
            .map_err(|source| crate::error::CanopyError::io("terminal", source))?;

        if state.should_quit {
            break Ok(());
        }

        if event::poll(Duration::from_millis(80))
            .map_err(|source| crate::error::CanopyError::io("terminal", source))?
        {
            if let Event::Key(key) =
                event::read().map_err(|source| crate::error::CanopyError::io("terminal", source))?
            {
                let (mut action, pending_g) = map_key_to_action(key, state.pending_g);
                state.pending_g = pending_g;

                if key.code == KeyCode::Enter {
                    action = match state.mode {
                        AppMode::Normal => KeyAction::Drill,
                        _ => KeyAction::Accept,
                    };
                }
                if key.code == KeyCode::Esc {
                    action = match state.mode {
                        AppMode::Normal => KeyAction::Back,
                        _ => KeyAction::Cancel,
                    };
                }
                if key.code == KeyCode::Backspace && matches!(state.mode, AppMode::Normal) {
                    action = KeyAction::Back;
                }

                if matches!(action, KeyAction::Noop) {
                    continue;
                }

                if let Err(err) = state.apply(action) {
                    state.status_line = format!("Error: {err}");
                }
            }
        }
    };

    disable_raw_mode().map_err(|source| crate::error::CanopyError::io("terminal", source))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|source| crate::error::CanopyError::io("terminal", source))?;
    terminal
        .show_cursor()
        .map_err(|source| crate::error::CanopyError::io("terminal", source))?;

    result
}

#[derive(Clone, Copy)]
struct Theme {
    panel_border: Color,
    panel_title: Color,
    text_primary: Color,
    text_secondary: Color,
    row_alt_bg: Color,
    row_selected_bg: Color,
    row_selected_fg: Color,
    status_bg: Color,
    status_accent: Color,
}

impl Theme {
    fn flight_deck() -> Self {
        Self {
            panel_border: Color::Rgb(95, 121, 168),
            panel_title: Color::Rgb(141, 196, 255),
            text_primary: Color::Rgb(226, 235, 255),
            text_secondary: Color::Rgb(147, 161, 199),
            row_alt_bg: Color::Rgb(24, 32, 57),
            row_selected_bg: Color::Rgb(47, 74, 124),
            row_selected_fg: Color::Rgb(245, 250, 255),
            status_bg: Color::Rgb(20, 27, 48),
            status_accent: Color::Rgb(74, 159, 255),
        }
    }

    fn kind_color(self, kind: NodeKind) -> Color {
        match kind {
            NodeKind::System => Color::Rgb(107, 203, 255),
            NodeKind::Container => Color::Rgb(123, 224, 161),
            NodeKind::Component => Color::Rgb(255, 192, 104),
            NodeKind::CodeUnit => Color::Rgb(188, 161, 255),
        }
    }

    fn kind_badge_style(self, kind: NodeKind) -> Style {
        Style::default()
            .fg(Color::Rgb(12, 15, 30))
            .bg(self.kind_color(kind))
            .add_modifier(Modifier::BOLD)
    }
}

fn render(frame: &mut ratatui::Frame<'_>, state: &AppState) {
    let theme = Theme::flight_deck();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(6)])
        .split(frame.area());

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(chunks[0]);

    render_tree(frame, body[0], state, theme, state.focus == FocusPane::Tree);
    render_details(
        frame,
        body[1],
        state,
        theme,
        state.focus == FocusPane::Semantic,
    );
    render_right_panel(
        frame,
        body[2],
        state,
        theme,
        state.focus == FocusPane::Right,
    );
    render_status(frame, chunks[1], state, theme);
    if state.mode == AppMode::Help {
        render_help_modal(frame, theme);
    }
}

fn render_tree(
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

fn render_details(
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

fn render_right_panel(
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

fn render_status(
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
        "?: help | / search | : query (Up/Down history) | e edit | r regenerate | v graph | i impact | H history | X export | PgUp/PgDn scroll right pane",
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

fn render_help_modal(frame: &mut ratatui::Frame<'_>, theme: Theme) {
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

fn kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::System => "SYS",
        NodeKind::Container => "CTR",
        NodeKind::Component => "CMP",
        NodeKind::CodeUnit => "CODE",
    }
}

fn confidence_color(confidence: f32) -> Color {
    if confidence >= 0.85 {
        Color::Rgb(124, 220, 143)
    } else if confidence >= 0.65 {
        Color::Rgb(255, 193, 107)
    } else {
        Color::Rgb(255, 131, 131)
    }
}

fn highlight_code_lines(contents: &str, theme: Theme) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    for raw_line in contents.lines().take(120) {
        let indent_len = raw_line
            .chars()
            .take_while(|ch| ch.is_ascii_whitespace())
            .count();
        let (indent, trimmed) = raw_line.split_at(indent_len);

        if trimmed.starts_with("#!") {
            out.push(Line::from(vec![Span::styled(
                raw_line.to_string(),
                Style::default().fg(Color::Rgb(206, 158, 255)),
            )]));
            continue;
        }
        if trimmed.starts_with('#') || trimmed.starts_with("//") {
            out.push(Line::from(vec![Span::styled(
                raw_line.to_string(),
                Style::default()
                    .fg(Color::Rgb(123, 170, 133))
                    .add_modifier(Modifier::ITALIC),
            )]));
            continue;
        }

        let keyword_style = |color: Color| Style::default().fg(color).add_modifier(Modifier::BOLD);
        let with_keyword = |kw: &str, color: Color| -> Option<Line<'static>> {
            if let Some(rest) = trimmed.strip_prefix(kw) {
                return Some(Line::from(vec![
                    Span::styled(
                        indent.to_string(),
                        Style::default().fg(theme.text_secondary),
                    ),
                    Span::styled(kw.to_string(), keyword_style(color)),
                    Span::styled(rest.to_string(), Style::default().fg(theme.text_primary)),
                ]));
            }
            None
        };

        let keyword_line = with_keyword("def ", Color::Rgb(255, 200, 120))
            .or_else(|| with_keyword("class ", Color::Rgb(255, 200, 120)))
            .or_else(|| with_keyword("fn ", Color::Rgb(255, 200, 120)))
            .or_else(|| with_keyword("pub fn ", Color::Rgb(255, 200, 120)))
            .or_else(|| with_keyword("struct ", Color::Rgb(255, 200, 120)))
            .or_else(|| with_keyword("enum ", Color::Rgb(255, 200, 120)))
            .or_else(|| with_keyword("impl ", Color::Rgb(255, 200, 120)))
            .or_else(|| with_keyword("import ", Color::Rgb(129, 209, 255)))
            .or_else(|| with_keyword("from ", Color::Rgb(129, 209, 255)))
            .or_else(|| with_keyword("use ", Color::Rgb(129, 209, 255)))
            .or_else(|| with_keyword("mod ", Color::Rgb(129, 209, 255)))
            .or_else(|| with_keyword("if ", Color::Rgb(182, 155, 255)))
            .or_else(|| with_keyword("for ", Color::Rgb(182, 155, 255)))
            .or_else(|| with_keyword("while ", Color::Rgb(182, 155, 255)))
            .or_else(|| with_keyword("match ", Color::Rgb(182, 155, 255)))
            .or_else(|| with_keyword("return ", Color::Rgb(182, 155, 255)));

        if let Some(line) = keyword_line {
            out.push(line);
            continue;
        }

        out.push(Line::from(vec![Span::styled(
            raw_line.to_string(),
            Style::default().fg(theme.text_primary),
        )]));
    }
    out
}
