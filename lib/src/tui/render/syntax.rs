use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme::Theme;

pub(super) fn highlight_code_lines(contents: &str, theme: Theme) -> Vec<Line<'static>> {
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
