use ratatui::prelude::*;
use ratatui::widgets::*;

use super::theme::Theme;

pub fn draw_header(frame: &mut Frame, area: Rect) {
    let outer_block = Block::default()
        .style(Style::default().bg(Theme::HEADER_BG))
        .borders(Borders::BOTTOM)
        .border_style(
            Style::default()
                .fg(Theme::BORDER)
                .add_modifier(Modifier::DIM),
        );

    let inner = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    let vert_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Min(1),
            Constraint::Percentage(50),
        ])
        .split(inner);

    let title = Line::from(vec![
        Span::styled(" ", Style::default().fg(Theme::MAUVE)),
        Span::styled("  ", Style::default().fg(Theme::MAUVE)),
        Span::styled(
            "━━━━━━━━━━━━",
            Style::default()
                .fg(Theme::BORDER)
                .add_modifier(Modifier::DIM),
        ),
        Span::styled(
            "  Rust-powered ⚡ YEGIN ⚡ Download Manager  ",
            Style::default()
                .fg(Theme::TEXT_DIM)
                .add_modifier(Modifier::ITALIC),
        ),
        Span::styled(
            "━━━━━━━━━━━━",
            Style::default()
                .fg(Theme::BORDER)
                .add_modifier(Modifier::DIM),
        ),
    ]);

    let title_para = Paragraph::new(title)
        .alignment(Alignment::Center)
        .style(Style::default().bg(Theme::HEADER_BG));
    frame.render_widget(title_para, vert_layout[1]);
}
