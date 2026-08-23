use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{App, DownloadStatus, InputMode};
use super::theme::Theme;
use super::utils::format_speed;

pub fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .style(Style::default().bg(Theme::BG_DARKER))
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Theme::BORDER));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let footer_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    let left = if let Some(ref msg) = app.status_message {
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("● ", Style::default().fg(Theme::GREEN)),
            Span::styled(msg.as_str(), Style::default().fg(Theme::GREEN)),
        ])
    } else {
        let active = app.downloads.iter().filter(|d| d.status == DownloadStatus::Downloading).count();
        let queued = app.downloads.iter().filter(|d| d.status == DownloadStatus::Pending).count();
        let total_speed: f64 = app.downloads.iter().filter(|d| d.status == DownloadStatus::Downloading).map(|d| d.speed).sum();
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(format!("⟳ {}/{}", active, app.max_concurrent), Style::default().fg(Theme::BLUE).add_modifier(Modifier::BOLD)),
            Span::styled(" active", Style::default().fg(Theme::TEXT_DIM)),
            if queued > 0 { Span::styled(format!("  │  ◷ {} queued", queued), Style::default().fg(Theme::YELLOW)) } else { Span::styled("", Style::default()) },
            Span::styled(format!("  │  ↓ {}/s", format_speed(total_speed)), Style::default().fg(Theme::TEXT_DIM)),
        ])
    };
    frame.render_widget(Paragraph::new(left), footer_layout[0]);

    let right = footer_shortcuts(footer_layout[1].width);
    frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), footer_layout[1]);

    // Second row: context-sensitive hints when in a modal
    if let Some(context_hints) = context_hint_line(app) {
        let context_area = Rect { x: inner.x, y: inner.y + 1, width: inner.width, height: 1 };
        if context_area.y + context_area.height <= area.y + area.height {
            frame.render_widget(Paragraph::new(context_hints).alignment(Alignment::Center), context_area);
        }
    }
}

fn context_hint_line(app: &App) -> Option<Line<'static>> {
    match app.input_mode {
        InputMode::Normal => None,
        InputMode::AddingUrl | InputMode::AddingName | InputMode::AddingSha256 | InputMode::RenameFile | InputMode::SettingSpeedLimit | InputMode::SettingConcurrency | InputMode::SettingField(_) => {
            Some(Line::from(vec![
                Span::styled("  Enter", Style::default().fg(Theme::GREEN).add_modifier(Modifier::BOLD)),
                Span::styled(":Confirm ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled("Esc", Style::default().fg(Theme::RED).add_modifier(Modifier::BOLD)),
                Span::styled(":Cancel ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled("Backspace", Style::default().fg(Theme::BLUE).add_modifier(Modifier::BOLD)),
                Span::styled(":Delete char", Style::default().fg(Theme::TEXT_DIM)),
            ]))
        }
        InputMode::Previewing => {
            Some(Line::from(vec![
                Span::styled("  Enter", Style::default().fg(Theme::GREEN).add_modifier(Modifier::BOLD)),
                Span::styled(":Start download ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled("Esc", Style::default().fg(Theme::RED).add_modifier(Modifier::BOLD)),
                Span::styled(":Cancel", Style::default().fg(Theme::TEXT_DIM)),
            ]))
        }
        InputMode::ConfirmDelete => {
            Some(Line::from(vec![
                Span::styled("  y", Style::default().fg(Theme::GREEN).add_modifier(Modifier::BOLD)),
                Span::styled(":Confirm delete ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled("n", Style::default().fg(Theme::RED).add_modifier(Modifier::BOLD)),
                Span::styled(":Cancel", Style::default().fg(Theme::TEXT_DIM)),
            ]))
        }
        InputMode::Help => {
            Some(Line::from(vec![
                Span::styled("  ↑↓", Style::default().fg(Theme::BLUE).add_modifier(Modifier::BOLD)),
                Span::styled(":Scroll  ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled("? / Esc", Style::default().fg(Theme::TEAL).add_modifier(Modifier::BOLD)),
                Span::styled(":Close", Style::default().fg(Theme::TEXT_DIM)),
            ]))
        }
        InputMode::DirectoryBrowser => {
            Some(Line::from(vec![
                Span::styled("  ↑↓", Style::default().fg(Theme::BLUE).add_modifier(Modifier::BOLD)),
                Span::styled(":Navigate ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled("Enter", Style::default().fg(Theme::GREEN).add_modifier(Modifier::BOLD)),
                Span::styled(":Open dir ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled("Backspace", Style::default().fg(Theme::BLUE).add_modifier(Modifier::BOLD)),
                Span::styled(":Go up ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled("y", Style::default().fg(Theme::GREEN).add_modifier(Modifier::BOLD)),
                Span::styled(":Select ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled("PgUp/PgDn", Style::default().fg(Theme::BLUE).add_modifier(Modifier::BOLD)),
                Span::styled(":Scroll ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled("Esc", Style::default().fg(Theme::RED).add_modifier(Modifier::BOLD)),
                Span::styled(":Cancel", Style::default().fg(Theme::TEXT_DIM)),
            ]))
        }
        InputMode::StatsDashboard => {
            Some(Line::from(vec![
                Span::styled("  ↑↓", Style::default().fg(Theme::BLUE).add_modifier(Modifier::BOLD)),
                Span::styled(":Navigate ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled("PgUp/PgDn", Style::default().fg(Theme::BLUE).add_modifier(Modifier::BOLD)),
                Span::styled(":Scroll ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled("g/G", Style::default().fg(Theme::BLUE).add_modifier(Modifier::BOLD)),
                Span::styled(":Top/Bottom ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled("Esc", Style::default().fg(Theme::RED).add_modifier(Modifier::BOLD)),
                Span::styled(":Close", Style::default().fg(Theme::TEXT_DIM)),
            ]))
        }
        InputMode::Settings => {
            Some(Line::from(vec![
                Span::styled("  ↑↓", Style::default().fg(Theme::BLUE).add_modifier(Modifier::BOLD)),
                Span::styled(":Navigate ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled("Enter", Style::default().fg(Theme::GREEN).add_modifier(Modifier::BOLD)),
                Span::styled(":Edit field ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled("Esc", Style::default().fg(Theme::RED).add_modifier(Modifier::BOLD)),
                Span::styled(":Close", Style::default().fg(Theme::TEXT_DIM)),
            ]))
        }
    }
}

fn footer_shortcuts(width: u16) -> Line<'static> {
    let bindings: &[(&str, &str, Color)] = if width >= 68 {
        &[
            ("a", " Add  ", Theme::GREEN),
            ("s", " Start  ", Theme::GREEN),
            ("p", " Pause  ", Theme::YELLOW),
            ("d", " Delete  ", Theme::RED),
            ("?", " Help  ", Theme::TEAL),
            ("q", " Quit", Theme::RED),
        ]
    } else {
        &[
            ("a", " Add  ", Theme::GREEN),
            ("?", " Help  ", Theme::TEAL),
            ("q", " Quit", Theme::RED),
        ]
    };

    Line::from(
        bindings
            .iter()
            .flat_map(|(key, label, color)| {
                [
                    Span::styled(*key, Style::default().fg(*color).add_modifier(Modifier::BOLD)),
                    Span::styled(*label, Style::default().fg(Theme::TEXT_DIM)),
                ]
            })
            .collect::<Vec<_>>(),
    )
}
