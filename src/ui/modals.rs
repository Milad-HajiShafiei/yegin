use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{App, InputMode};
use super::theme::Theme;
use super::utils::{centered_rect, format_bytes, format_speed};

// ── Input Modal (URL, name, SHA-256, speed limit, concurrency, rename) ────

pub fn draw_input_modal(frame: &mut Frame, area: Rect, title: &str, value: &str) {
    let popup_area = centered_rect(45, 16, area);
    frame.render_widget(Clear, popup_area);
    frame.render_widget(
        Block::default().style(Style::default().bg(Theme::SHADOW)),
        popup_area,
    );

    let block = Block::default()
        .title(format!(" {} ", title))
        .title_style(Style::default().fg(Theme::BLUE).add_modifier(Modifier::BOLD))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Theme::BLUE).add_modifier(Modifier::BOLD))
        .style(Style::default().bg(Theme::MODAL_BG));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Content: 1 input line + 1 blank + 1 hint = 3 lines
    let content_h: u16 = 3;
    let pad_y = inner.height.saturating_sub(content_h) / 2;

    // Input line — left-aligned
    let input_display = if value.is_empty() {
        Span::styled("│ ", Style::default().fg(Theme::TEXT_MUTE))
    } else {
        Span::styled(format!("│ {} ", value), Style::default().fg(Theme::TEXT_BRIGHT))
    };
    let input_line = Line::from(vec![
        input_display,
        Span::styled("█", Style::default().fg(Theme::BLUE).add_modifier(Modifier::SLOW_BLINK)),
    ]);
    let input_para = Paragraph::new(input_line)
        .style(Style::default().bg(Theme::BG_LIGHTER).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Left);
    frame.render_widget(
        input_para,
        Rect { x: inner.x, y: inner.y + pad_y, width: inner.width, height: 1 },
    );

    // Hint — centered
    let hint = Paragraph::new(Line::from(vec![
        Span::styled(
            "Enter: Confirm  │  Esc: Cancel",
            Style::default().fg(Theme::TEXT_DIM).add_modifier(Modifier::ITALIC),
        ),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(
        hint,
        Rect { x: inner.x, y: inner.y + pad_y + 2, width: inner.width, height: 1 },
    );
}

// ── Confirm Delete Modal ───────────────────────────────────────────────────

pub fn draw_confirm_modal(frame: &mut Frame, area: Rect, app: &App) {
    let popup_area = centered_rect(40, 14, area);
    frame.render_widget(Clear, popup_area);
    frame.render_widget(
        Block::default().style(Style::default().bg(Theme::SHADOW)),
        popup_area,
    );

    let block = Block::default()
        .title(" ⚠ Confirm Delete ")
        .title_style(Style::default().fg(Theme::RED).add_modifier(Modifier::BOLD))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Theme::RED).add_modifier(Modifier::BOLD))
        .style(Style::default().bg(Theme::MODAL_BG));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let msg = if app.selected < app.downloads.len() {
        format!("Delete \"{}\"?", app.downloads[app.selected].filename)
    } else {
        "Delete this item?".to_string()
    };

    // Content: 1 message + 1 blank + 1 confirm = 3 lines
    let content_h: u16 = 3;
    let pad_y = inner.height.saturating_sub(content_h) / 2;

    // Message — centered
    let msg_para = Paragraph::new(Line::from(Span::styled(
        &msg,
        Style::default().fg(Theme::TEXT),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(
        msg_para,
        Rect { x: inner.x, y: inner.y + pad_y, width: inner.width, height: 1 },
    );

    // Confirm options — centered
    let confirm = Paragraph::new(Line::from(vec![
        Span::styled("y", Style::default().fg(Theme::RED).add_modifier(Modifier::BOLD)),
        Span::styled(": Yes   ", Style::default().fg(Theme::TEXT_DIM)),
        Span::styled("n", Style::default().fg(Theme::GREEN).add_modifier(Modifier::BOLD)),
        Span::styled(": No", Style::default().fg(Theme::TEXT_DIM)),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(
        confirm,
        Rect { x: inner.x, y: inner.y + pad_y + 2, width: inner.width, height: 1 },
    );
}

// ── Help Overlay ───────────────────────────────────────────────────────────

pub fn draw_help_overlay(frame: &mut Frame, area: Rect) {
    let popup_area = centered_rect(55, 65, area);
    frame.render_widget(Clear, popup_area);
    frame.render_widget(
        Block::default().style(Style::default().bg(Theme::SHADOW)),
        popup_area,
    );

    let block = Block::default()
        .title(" ⌨  Keyboard Shortcuts ")
        .title_style(Style::default().fg(Theme::TEAL).add_modifier(Modifier::BOLD))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Theme::TEAL).add_modifier(Modifier::BOLD))
        .style(Style::default().bg(Theme::MODAL_BG));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Add 1-char padding on left/right
    let padded = Rect {
        x: inner.x + 1,
        y: inner.y,
        width: inner.width.saturating_sub(2),
        height: inner.height,
    };

    let bindings = vec![
        ("", ""),
        ("Navigation", ""),
        ("  ↑ / k", "Move selection up"),
        ("  ↓ / j", "Move selection down"),
        ("  g / Home", "Go to top"),
        ("  G / End", "Go to bottom"),
        ("", ""),
        ("Downloads", ""),
        ("  a", "Add new download (URL)"),
        ("  s", "Start / resume selected"),
        ("  p", "Pause selected"),
        ("  r", "Resume paused download"),
        ("  t", "Retry failed download"),
        ("  d / Del", "Delete selected"),
        ("  x", "Clear completed/failed downloads"),
        ("  R", "Rename file"),
        ("  c", "Change download directory"),
        ("  Ctrl+V", "Paste URL from clipboard"),
        ("  v", "Verify SHA-256 checksum"),
        ("  L", "Set speed limit"),
        ("  S", "Open stats dashboard"),
        ("  M", "Set max concurrent downloads"),
        ("  o", "Open containing folder"),
        ("  :", "Open settings"),
        ("", ""),
        ("General", ""),
        ("  ?", "Toggle this help"),
        ("  q / Esc", "Quit / Close dialog"),
        ("", ""),
    ];

    let help_lines: Vec<Line> = bindings
        .iter()
        .map(|(key, desc)| {
            if desc.is_empty() && !key.is_empty() {
                Line::from(vec![Span::styled(
                    format!("  {}", key),
                    Style::default().fg(Theme::MAUVE).add_modifier(Modifier::BOLD),
                )])
            } else if key.is_empty() && desc.is_empty() {
                Line::from("")
            } else {
                Line::from(vec![
                    Span::styled(
                        format!("  {:<14}", key),
                        Style::default().fg(Theme::GREEN).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(*desc, Style::default().fg(Theme::TEXT)),
                ])
            }
        })
        .collect();

    frame.render_widget(Paragraph::new(help_lines), padded);
}

// ── Directory Browser ──────────────────────────────────────────────────────

pub fn draw_directory_browser(frame: &mut Frame, area: Rect, app: &App) {
    let popup_area = centered_rect(55, 65, area);
    frame.render_widget(Clear, popup_area);
    frame.render_widget(
        Block::default().style(Style::default().bg(Theme::SHADOW)),
        popup_area,
    );

    let block = Block::default()
        .title(format!(" 📁 {} ", app.browser_path.display()))
        .title_style(Style::default().fg(Theme::TEAL).add_modifier(Modifier::BOLD))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Theme::TEAL).add_modifier(Modifier::BOLD))
        .style(Style::default().bg(Theme::MODAL_BG));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let browser_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(1), Constraint::Length(2)])
        .split(inner);

    let list_area = browser_layout[0];

    if app.browser_entries.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "  (empty directory)",
                Style::default().fg(Theme::TEXT_DIM).add_modifier(Modifier::ITALIC),
            )]),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(empty, list_area);
    } else {
        let selected = app.browser_selected;
        let items: Vec<ListItem> = app
            .browser_entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let is_selected = i == selected;
                let bg = if is_selected { Theme::BG_CARD } else { Theme::MODAL_BG };
                let (icon, icon_color) = if entry.is_dir {
                    ("📂 ", Theme::YELLOW)
                } else {
                    ("📄 ", Theme::TEXT_DIM)
                };
                let name_color = if is_selected {
                    Theme::TEXT_BRIGHT
                } else if entry.is_dir {
                    Theme::BLUE
                } else {
                    Theme::TEXT
                };
                let name_style = if is_selected {
                    Style::default().fg(name_color).bg(bg).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(name_color).bg(bg)
                };

                let mut spans = vec![
                    Span::styled(icon, Style::default().fg(icon_color).bg(bg)),
                    Span::styled(entry.name.clone(), name_style),
                ];
                if !entry.is_dir {
                    if let Some(ext) = entry.path.extension() {
                        spans.push(Span::styled(
                            format!("  [{}]", ext.to_string_lossy().to_uppercase()),
                            Style::default().fg(Theme::TEXT_DIM).bg(bg),
                        ));
                    }
                } else {
                    spans.push(Span::styled("  ", Style::default().bg(bg)));
                }
                ListItem::new(Line::from(spans)).style(Style::default().bg(bg))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(Theme::BG_CARD)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(" ");
        let mut state = ListState::default();
        state.select(Some(selected));
        frame.render_stateful_widget(list, list_area, &mut state);
    }

    let sep = Paragraph::new(Line::from(vec![Span::styled(
        "─".repeat(inner.width as usize),
        Style::default().fg(Theme::BORDER),
    )]));
    frame.render_widget(sep, browser_layout[1]);

    let hints = Paragraph::new(Line::from(vec![
        Span::styled(
            "Enter: Open  ",
            Style::default().fg(Theme::GREEN).add_modifier(Modifier::BOLD),
        ),
        Span::styled("↑↓: Navigate  ", Style::default().fg(Theme::TEXT_DIM)),
        Span::styled("Backspace: Go up  ", Style::default().fg(Theme::YELLOW)),
        Span::styled("Esc: Cancel", Style::default().fg(Theme::RED)),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(hints, browser_layout[2]);
}

// ── Stats Dashboard (excluded from centering changes) ──────────────────────

pub fn draw_stats_dashboard(frame: &mut Frame, area: Rect, app: &App) {
    let popup_area = centered_rect(78, 78, area);
    let dim = Block::default().style(Style::default().bg(Theme::SHADOW).add_modifier(Modifier::DIM));
    frame.render_widget(Clear, popup_area);
    frame.render_widget(dim, popup_area);

    let block = Block::default()
        .title(" 📊 Statistics Dashboard ")
        .title_style(Style::default().fg(Theme::MAUVE).add_modifier(Modifier::BOLD))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Theme::MAUVE).add_modifier(Modifier::BOLD))
        .style(Style::default().bg(Theme::MODAL_BG));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let dashboard_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Length(1), Constraint::Min(5), Constraint::Length(1)])
        .split(inner);

    let cards_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(25), Constraint::Percentage(25), Constraint::Percentage(25)])
        .split(dashboard_layout[0]);

    let session_secs = app.session_start.elapsed().as_secs();
    let session_str = format!("{}h {}m {}s", session_secs / 3600, (session_secs % 3600) / 60, session_secs % 60);
    let avg_speed = if session_secs > 0 { app.total_bytes_downloaded as f64 / session_secs as f64 } else { 0.0 };

    let card_style = |border_color: Color| Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(border_color)).style(Style::default().bg(Theme::BG));

    let card1 = Paragraph::new(vec![
        Line::from(vec![Span::styled("  📦 Total Downloaded", Style::default().fg(Theme::TEAL).add_modifier(Modifier::BOLD))]),
        Line::from(""),
        Line::from(vec![Span::styled(format!("  {}", format_bytes(app.total_bytes_downloaded)), Style::default().fg(Theme::TEXT_BRIGHT).add_modifier(Modifier::BOLD))]),
    ]).block(card_style(Theme::TEAL));
    frame.render_widget(card1, cards_layout[0]);

    let card2 = Paragraph::new(vec![
        Line::from(vec![Span::styled("  ✅ Files Completed", Style::default().fg(Theme::GREEN).add_modifier(Modifier::BOLD))]),
        Line::from(""),
        Line::from(vec![Span::styled(format!("  {}", app.total_completed), Style::default().fg(Theme::TEXT_BRIGHT).add_modifier(Modifier::BOLD))]),
    ]).block(card_style(Theme::GREEN));
    frame.render_widget(card2, cards_layout[1]);

    let card3 = Paragraph::new(vec![
        Line::from(vec![Span::styled("  ⚡ Avg Speed", Style::default().fg(Theme::YELLOW).add_modifier(Modifier::BOLD))]),
        Line::from(""),
        Line::from(vec![Span::styled(format!("  {}/s", format_speed(avg_speed)), Style::default().fg(Theme::TEXT_BRIGHT).add_modifier(Modifier::BOLD))]),
    ]).block(card_style(Theme::YELLOW));
    frame.render_widget(card3, cards_layout[2]);

    let card4 = Paragraph::new(vec![
        Line::from(vec![Span::styled("  ⏱ Session", Style::default().fg(Theme::ORANGE).add_modifier(Modifier::BOLD))]),
        Line::from(""),
        Line::from(vec![Span::styled(format!("  {}", session_str), Style::default().fg(Theme::TEXT_BRIGHT).add_modifier(Modifier::BOLD))]),
    ]).block(card_style(Theme::ORANGE));
    frame.render_widget(card4, cards_layout[3]);

    let sep = Paragraph::new(Line::from(vec![Span::styled("─".repeat(inner.width as usize), Style::default().fg(Theme::BORDER))]));
    frame.render_widget(sep, dashboard_layout[1]);

    let history_area = dashboard_layout[2];
    if app.history.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![Span::styled("  No download history yet", Style::default().fg(Theme::TEXT_DIM).add_modifier(Modifier::ITALIC))]),
        ]).alignment(Alignment::Center);
        frame.render_widget(empty, history_area);
    } else {
        let header = Line::from(vec![
            Span::styled("  File", Style::default().fg(Theme::MAUVE).add_modifier(Modifier::BOLD)),
            Span::styled("                                         ", Style::default()),
            Span::styled("Size        ", Style::default().fg(Theme::MAUVE).add_modifier(Modifier::BOLD)),
            Span::styled("Duration  ", Style::default().fg(Theme::MAUVE).add_modifier(Modifier::BOLD)),
            Span::styled("Completed", Style::default().fg(Theme::MAUVE).add_modifier(Modifier::BOLD)),
        ]);

        let visible_height = history_area.height as usize;
        let total = app.history.len();
        let scroll = app.history_scroll.min(total.saturating_sub(1));

        let mut rows: Vec<Line> = vec![header];
        for (i, entry) in app.history.iter().enumerate().skip(scroll).take(visible_height.saturating_sub(1)) {
            let is_selected = i == scroll;
            let bg = if is_selected { Theme::BG_CARD } else { Theme::MODAL_BG };
            let name_max = 38;
            let display_name = if entry.filename.len() > name_max { format!("{}…", &entry.filename[..name_max - 1]) } else { entry.filename.clone() };
            let dur_str = if entry.duration_secs >= 3600 { format!("{}h {}m", entry.duration_secs / 3600, (entry.duration_secs % 3600) / 60) }
                else if entry.duration_secs >= 60 { format!("{}m {}s", entry.duration_secs / 60, entry.duration_secs % 60) }
                else { format!("{}s", entry.duration_secs) };

            rows.push(Line::from(vec![
                Span::styled(format!("  {:<width$}", display_name, width = name_max), Style::default().fg(if is_selected { Theme::TEXT_BRIGHT } else { Theme::TEXT }).bg(bg)),
                Span::styled(format!("{:>12}  ", format_bytes(entry.size_bytes)), Style::default().fg(Theme::TEAL).bg(bg)),
                Span::styled(format!("{:<10}", dur_str), Style::default().fg(Theme::ORANGE).bg(bg)),
                Span::styled(&entry.completed_at, Style::default().fg(Theme::TEXT_DIM).bg(bg)),
            ]));
        }

        let history_list = List::new(rows).highlight_style(Style::default().bg(Theme::BG_CARD)).highlight_symbol("");
        let mut state = ListState::default();
        state.select(Some(0));
        frame.render_stateful_widget(history_list, history_area, &mut state);
    }

    let hints = Line::from(vec![
        Span::styled("↑↓: Scroll  ", Style::default().fg(Theme::TEXT_DIM)),
        Span::styled("Esc: Close", Style::default().fg(Theme::RED)),
    ]);
    frame.render_widget(Paragraph::new(hints).alignment(Alignment::Center), dashboard_layout[3]);
}

// ── Settings Modal ─────────────────────────────────────────────────────────

pub fn draw_settings_modal(frame: &mut Frame, area: Rect, app: &App) {
    let popup_area = centered_rect(50, 18, area);
    frame.render_widget(Clear, popup_area);
    frame.render_widget(
        Block::default().style(Style::default().bg(Theme::SHADOW)),
        popup_area,
    );

    let block = Block::default()
        .title(" ⚙ Settings ")
        .title_style(Style::default().fg(Theme::ORANGE).add_modifier(Modifier::BOLD))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Theme::ORANGE).add_modifier(Modifier::BOLD))
        .style(Style::default().bg(Theme::MODAL_BG));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let editing = matches!(app.input_mode, InputMode::SettingField(_));
    let sel = app.settings_selected;

    let fields = [
        "Max concurrent downloads",
        "Default speed limit",
        "Download directory",
        "Auto-verify SHA-256",
    ];
    let default_values = [
        format!("{}", app.settings.max_concurrent),
        match app.settings.default_speed_limit {
            Some(bps) => format!("{}/s", format_speed(bps as f64)),
            None => "Unlimited".to_string(),
        },
        app.settings.download_dir.display().to_string(),
        if app.settings.auto_verify { "On".to_string() } else { "Off".to_string() },
    ];

    // Content: 4 field lines + 1 blank + 1 hint = 6 lines
    let content_h: u16 = 6;
    let pad_y = inner.height.saturating_sub(content_h) / 2;

    let mut lines: Vec<Line> = vec![];
    for (i, (field, default_val)) in fields.iter().zip(default_values.iter()).enumerate() {
        let is_selected = i == sel;
        let is_editing = editing && i == sel;
        let display_value = if is_editing { format!("{}█", app.input_buf) } else { default_val.clone() };
        let bg = if is_selected { Theme::BG_CARD } else { Theme::MODAL_BG };
        let field_color = if is_selected { Theme::BLUE } else { Theme::TEXT };
        let value_color = if is_editing { Theme::GREEN } else if is_selected { Theme::TEXT_BRIGHT } else { Theme::TEXT_DIM };
        let icon = match i {
            0 => "🔀 ",
            1 => "⚡ ",
            2 => "📁 ",
            3 => "🔍 ",
            _ => "  ",
        };
        let indicator = if is_editing {
            " "
        } else if is_selected {
            " →"
        } else {
            ""
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!("  {}{}", icon, field),
                Style::default().fg(field_color).bg(bg).add_modifier(if is_selected { Modifier::BOLD } else { Modifier::empty() }),
            ),
            Span::styled("   ", Style::default().bg(bg)),
            Span::styled(
                format!("{}{}", display_value, indicator),
                Style::default().fg(value_color).bg(bg).add_modifier(if is_editing { Modifier::BOLD } else { Modifier::empty() }),
            ),
        ]));
    }

    // Fields — left-aligned
    let list_para = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(
        list_para,
        Rect { x: inner.x, y: inner.y + pad_y, width: inner.width, height: fields.len() as u16 },
    );

    // Hint — left-aligned
    let hint = if editing {
        Line::from(vec![Span::styled(
            "  Enter: Confirm  │  Esc: Cancel",
            Style::default().fg(Theme::TEXT_DIM).add_modifier(Modifier::ITALIC),
        )])
    } else {
        Line::from(vec![Span::styled(
            "  ↑↓: Navigate  │  Enter: Edit  │  Esc: Close",
            Style::default().fg(Theme::TEXT_DIM).add_modifier(Modifier::ITALIC),
        )])
    };
    let hint_para = Paragraph::new(hint).alignment(Alignment::Left);
    frame.render_widget(
        hint_para,
        Rect { x: inner.x, y: inner.y + pad_y + fields.len() as u16 + 1, width: inner.width, height: 1 },
    );
}

// ── Preview Modal ──────────────────────────────────────────────────────────

pub fn draw_preview_modal(frame: &mut Frame, area: Rect, app: &App) {
    let popup_area = centered_rect(50, 28, area);
    frame.render_widget(Clear, popup_area);
    frame.render_widget(
        Block::default().style(Style::default().bg(Theme::SHADOW)),
        popup_area,
    );

    let block = Block::default()
        .title(" 📋 Download Preview ")
        .title_style(Style::default().fg(Theme::MAUVE).add_modifier(Modifier::BOLD))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Theme::MAUVE))
        .style(Style::default().bg(Theme::MODAL_BG));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let preview = match &app.preview {
        Some(p) => p,
        None => {
            // Loading state — center vertically
            let content_h: u16 = 5;
            let pad_y = inner.height.saturating_sub(content_h) / 2;
            let loading = Paragraph::new(vec![
                Line::from(vec![Span::styled(
                    "⏳ Fetching file info...",
                    Style::default().fg(Theme::BLUE).add_modifier(Modifier::ITALIC),
                )]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "Sending HEAD request to server",
                    Style::default().fg(Theme::TEXT_DIM),
                )]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "Press Enter when ready",
                    Style::default().fg(Theme::TEXT_DIM).add_modifier(Modifier::ITALIC),
                )]),
            ])
            .alignment(Alignment::Center);
            frame.render_widget(
                loading,
                Rect { x: inner.x, y: inner.y + pad_y, width: inner.width, height: content_h },
            );
            return;
        }
    };

    // Content: URL + File + divider + Size + Type + Ranges + SaveTo + divider + blank + Buttons = 10 lines
    let content_h: u16 = 10;
    let pad_y = inner.height.saturating_sub(content_h) / 2;

    let mut y = inner.y + pad_y;

    // URL
    let max_url_len = inner.width.saturating_sub(14) as usize;
    let display_url = if preview.url.len() > max_url_len {
        format!("{}…", &preview.url[..max_url_len - 1])
    } else {
        preview.url.clone()
    };
    render_preview_line(frame, &mut y, inner.x, inner.width, PreviewLineParams { icon: "🔗", icon_color: Theme::TEAL, label: "URL", value: &display_url, value_color: Theme::TEXT_DIM });

    // Filename
    render_preview_line(frame, &mut y, inner.x, inner.width, PreviewLineParams { icon: "📄", icon_color: Theme::ORANGE, label: "File", value: &preview.filename, value_color: Theme::TEXT_BRIGHT });

    // Divider
    let divider = Line::from(vec![Span::styled(
        "  ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄",
        Style::default().fg(Theme::BORDER).add_modifier(Modifier::DIM),
    )]);
    frame.render_widget(Paragraph::new(divider), Rect { x: inner.x, y, width: inner.width, height: 1 });
    y += 1;

    // Size
    let size_text = match preview.total_bytes {
        Some(bytes) => format!("{} ({} bytes)", format_bytes(bytes), bytes),
        None => "Unknown".to_string(),
    };
    let size_color = match preview.total_bytes {
        Some(b) if b > 100 * 1024 * 1024 => Theme::YELLOW,
        Some(_) => Theme::GREEN,
        None => Theme::TEXT_DIM,
    };
    render_preview_line(frame, &mut y, inner.x, inner.width, PreviewLineParams { icon: "📦", icon_color: Theme::PURPLE, label: "Size", value: &size_text, value_color: size_color });

    // Content Type
    let ct_text = preview.content_type.as_deref().unwrap_or("Unknown");
    render_preview_line(frame, &mut y, inner.x, inner.width, PreviewLineParams { icon: "🏷", icon_color: Theme::SKY, label: "Type", value: ct_text, value_color: Theme::TEXT });

    // Range Support
    let range_text = if preview.supports_range {
        "Yes (parallel download)"
    } else {
        "No (single stream)"
    };
    let range_color = if preview.supports_range { Theme::GREEN } else { Theme::TEXT_DIM };
    render_preview_line(frame, &mut y, inner.x, inner.width, PreviewLineParams { icon: "⚡", icon_color: Theme::TEAL, label: "Ranges", value: range_text, value_color: range_color });

    // Destination
    let dest = app.download_dir.join(&preview.filename);
    let dest_str = dest.display().to_string();
    let max_dest_len = inner.width.saturating_sub(14) as usize;
    let display_dest = if dest_str.len() > max_dest_len {
        format!("…{}", &dest_str[dest_str.len() - max_dest_len + 1..])
    } else {
        dest_str
    };
    render_preview_line(frame, &mut y, inner.x, inner.width, PreviewLineParams { icon: "💾", icon_color: Theme::PINK, label: "Save to", value: &display_dest, value_color: Theme::TEXT_DIM });

    // Divider
    y += 1;
    let divider2 = Line::from(vec![Span::styled(
        "  ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄",
        Style::default().fg(Theme::BORDER).add_modifier(Modifier::DIM),
    )]);
    frame.render_widget(Paragraph::new(divider2), Rect { x: inner.x, y, width: inner.width, height: 1 });
    y += 1;

    // Action buttons — horizontally centered
    let buttons = Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(
            " [Enter] Start Download ",
            Style::default().fg(Theme::BG_DARKER).bg(Theme::GREEN).add_modifier(Modifier::BOLD),
        ),
        Span::styled("   ", Style::default()),
        Span::styled(
            " [Esc] Cancel ",
            Style::default().fg(Theme::BG_DARKER).bg(Theme::RED).add_modifier(Modifier::BOLD),
        ),
    ]);
    let btn_para = Paragraph::new(buttons).alignment(Alignment::Center);
    frame.render_widget(btn_para, Rect { x: inner.x, y, width: inner.width, height: 1 });
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Visual parameters for a preview info line.
struct PreviewLineParams<'a> {
    icon: &'a str,
    icon_color: Color,
    label: &'a str,
    value: &'a str,
    value_color: Color,
}

fn render_preview_line(frame: &mut Frame, y: &mut u16, x: u16, width: u16, p: PreviewLineParams<'_>) {
    let line = Line::from(vec![
        Span::styled(format!("  {} ", p.icon), Style::default().fg(p.icon_color)),
        Span::styled(
            format!("{}: ", p.label),
            Style::default().fg(Theme::TEXT_DIM).add_modifier(Modifier::BOLD),
        ),
        Span::styled(p.value.to_string(), Style::default().fg(p.value_color)),
    ]);
    frame.render_widget(Paragraph::new(line), Rect { x, y: *y, width, height: 1 });
    *y += 1;
}
