use ratatui::style::{Color, Modifier, Style};

use crate::domain::NodeKind;

#[derive(Clone, Copy)]
pub(super) struct Theme {
    pub(super) panel_border: Color,
    pub(super) panel_title: Color,
    pub(super) text_primary: Color,
    pub(super) text_secondary: Color,
    pub(super) row_alt_bg: Color,
    pub(super) row_selected_bg: Color,
    pub(super) row_selected_fg: Color,
    pub(super) status_bg: Color,
    pub(super) status_accent: Color,
}

impl Theme {
    pub(super) fn flight_deck() -> Self {
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

    pub(super) fn kind_color(self, kind: NodeKind) -> Color {
        match kind {
            NodeKind::System => Color::Rgb(107, 203, 255),
            NodeKind::Container => Color::Rgb(123, 224, 161),
            NodeKind::Component => Color::Rgb(255, 192, 104),
            NodeKind::CodeUnit => Color::Rgb(188, 161, 255),
        }
    }

    pub(super) fn kind_badge_style(self, kind: NodeKind) -> Style {
        Style::default()
            .fg(Color::Rgb(12, 15, 30))
            .bg(self.kind_color(kind))
            .add_modifier(Modifier::BOLD)
    }
}

pub(super) fn kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::System => "SYS",
        NodeKind::Container => "CTR",
        NodeKind::Component => "CMP",
        NodeKind::CodeUnit => "CODE",
    }
}

pub(super) fn confidence_color(confidence: f32) -> Color {
    if confidence >= 0.85 {
        Color::Rgb(124, 220, 143)
    } else if confidence >= 0.65 {
        Color::Rgb(255, 193, 107)
    } else {
        Color::Rgb(255, 131, 131)
    }
}
