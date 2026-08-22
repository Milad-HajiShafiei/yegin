use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{App, DownloadStatus, VerifyStatus};
use super::theme::Theme;
use super::utils::{fade_color, format_bytes, format_speed, lerp_channel, color_to_rgb, render_detail_line, InfoLineParams};

pub fn draw_detail_panel(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" 📋 Details ")
        .title_style(Style::default().fg(Theme::MAUVE).add_modifier(Modifier::BOLD))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::BORDER))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.downloads.is_empty() || app.selected >= app.downloads.len() {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![Span::styled("  Select a download", Style::default().fg(Theme::TEXT_DIM).add_modifier(Modifier::ITALIC))]),
            Line::from(vec![Span::styled("  to view details", Style::default().fg(Theme::TEXT_DIM).add_modifier(Modifier::ITALIC))]),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(empty, inner);
        return;
    }

    let dl = &app.downloads[app.selected];

    // Fade-in animation (pure fade, no slide)
    let fade_ms = app.last_selection_change.elapsed().as_millis() as f64;
    let fade_duration = 250.0;
    let fade_factor = if fade_ms < fade_duration {
        let t = fade_ms / fade_duration;
        // Ease-out cubic
        1.0 - (1.0 - t).powi(3)
    } else {
        1.0
    };

    // Filename Header
    let fname = Line::from(vec![
        Span::styled("  📄 ", Style::default().fg(fade_color(Theme::ORANGE, fade_factor))),
        Span::styled(&dl.filename, Style::default().fg(fade_color(Theme::TEXT_BRIGHT, fade_factor)).add_modifier(Modifier::BOLD)),
    ]);
    frame.render_widget(Paragraph::new(fname), Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 });

    // Divider
    let divider = Line::from(vec![Span::styled("  ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄", Style::default().fg(fade_color(Theme::BORDER, fade_factor)).add_modifier(Modifier::DIM))]);
    frame.render_widget(Paragraph::new(divider), Rect { x: inner.x, y: inner.y + 1, width: inner.width, height: 1 });

    let info_start = inner.y + 2;

    // URL
    let max_url_len = inner.width.saturating_sub(8) as usize;
    let display_url = if dl.url.len() > max_url_len { format!("{}…", &dl.url[..max_url_len - 1]) } else { dl.url.clone() };
    render_detail_line(frame, Rect { x: inner.x, y: info_start, width: inner.width, height: 1 }, InfoLineParams { icon: "🔗", icon_color: Theme::TEAL, label: "URL", value: &display_url, value_color: Theme::TEXT_DIM, fade: fade_factor });

    // Progress
    let size_text = if dl.has_total() {
        format!("{:.1}%  ({} / {})", dl.progress(), format_bytes(dl.downloaded_bytes), format_bytes(dl.total_bytes.unwrap()))
    } else if dl.speed > 0.0 {
        format!("{} downloaded (size unknown)", format_bytes(dl.downloaded_bytes))
    } else {
        format!("{} downloaded", format_bytes(dl.downloaded_bytes))
    };
    render_detail_line(frame, Rect { x: inner.x, y: info_start + 1, width: inner.width, height: 1 }, InfoLineParams { icon: "📐", icon_color: Theme::PURPLE, label: "Progress", value: &size_text, value_color: Theme::TEXT, fade: fade_factor });

    // Status
    let (status_text, status_color) = match &dl.status {
        DownloadStatus::Pending => ("◷ Pending", Theme::TEXT_DIM),
        DownloadStatus::Downloading => ("↓ Downloading", Theme::BLUE),
        DownloadStatus::Paused => ("❙❙ Paused", Theme::YELLOW),
        DownloadStatus::Completed => ("✓ Completed", Theme::GREEN),
        DownloadStatus::Failed(_) => ("✗ Failed", Theme::RED),
    };
    render_detail_line(frame, Rect { x: inner.x, y: info_start + 2, width: inner.width, height: 1 }, InfoLineParams { icon: "📊", icon_color: Theme::YELLOW, label: "Status", value: status_text, value_color: status_color, fade: fade_factor });

    // Speed with breathing pulse
    let speed_area = Rect { x: inner.x, y: info_start + 3, width: inner.width, height: 1 };
    if dl.speed > 0.0 && dl.status == DownloadStatus::Downloading {
        let elapsed_ms = dl.start_time.map(|s| s.elapsed().as_millis()).unwrap_or(0);
        let pulse_cycle = 1800.0;
        let t = (elapsed_ms as f64 % pulse_cycle) / pulse_cycle;
        let pulse = (t * std::f64::consts::TAU).sin() * 0.225 + 0.775;
        let (br, bg_c, bb) = color_to_rgb(Theme::TEAL);
        let r = lerp_channel(br as f64, 245.0, pulse * 0.5);
        let g = lerp_channel(bg_c as f64, 250.0, pulse * 0.5);
        let b = lerp_channel(bb as f64, 255.0, pulse * 0.5);
        let speed_color = Color::Rgb(r as u8, g as u8, b as u8);
        let speed_line = Line::from(vec![
            Span::styled("  ⚡ ", Style::default().fg(fade_color(Theme::GREEN, fade_factor))),
            Span::styled("Speed: ", Style::default().fg(fade_color(Theme::TEXT_DIM, fade_factor)).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} /s", format_speed(dl.speed)), Style::default().fg(fade_color(speed_color, fade_factor)).add_modifier(Modifier::BOLD)),
        ]);
        frame.render_widget(Paragraph::new(speed_line), speed_area);
    } else {
        let speed_text = if dl.speed > 0.0 { format!("{} /s", format_speed(dl.speed)) } else { "—".to_string() };
        render_detail_line(frame, speed_area, InfoLineParams { icon: "⚡", icon_color: Theme::GREEN, label: "Speed", value: &speed_text, value_color: Theme::TEXT, fade: fade_factor });
    }

    // Speed Limit
    let limit_text = match dl.speed_limit { Some(bps) => format!("{}/s", format_speed(bps as f64)), None => "Unlimited".to_string() };
    let limit_color = if dl.speed_limit.is_some() { Theme::YELLOW } else { Theme::TEXT_DIM };
    render_detail_line(frame, Rect { x: inner.x, y: info_start + 4, width: inner.width, height: 1 }, InfoLineParams { icon: "🐢", icon_color: Theme::YELLOW, label: "Limit", value: &limit_text, value_color: limit_color, fade: fade_factor });

    // Chunks
    let chunks_text = if dl.num_chunks > 1 {
        let parts: Vec<String> = dl.chunk_downloaded.iter().enumerate().map(|(i, &_bytes)| {
            let cp = dl.chunk_progress(i);
            if cp >= 100.0 { format!("c{}:✓", i + 1) } else if cp > 0.0 { format!("c{}:{:.0}%", i + 1, cp) } else { format!("c{}:…", i + 1) }
        }).collect();
        format!("{} chunks [{}]", dl.num_chunks, parts.join(" "))
    } else if dl.supports_range { "1 chunk (range supported)".to_string() } else { "1 chunk (streaming)".to_string() };
    render_detail_line(frame, Rect { x: inner.x, y: info_start + 5, width: inner.width, height: 1 }, InfoLineParams { icon: "🧩", icon_color: Theme::TEAL, label: "Chunks", value: &chunks_text, value_color: Theme::TEXT, fade: fade_factor });

    // Elapsed
    let elapsed_text = if let Some(start) = dl.start_time { format!("{}s", start.elapsed().as_secs()) } else if dl.elapsed.as_secs() > 0 { format!("{}s", dl.elapsed.as_secs()) } else { "—".to_string() };
    render_detail_line(frame, Rect { x: inner.x, y: info_start + 6, width: inner.width, height: 1 }, InfoLineParams { icon: "⏱", icon_color: Theme::ORANGE, label: "Elapsed", value: &elapsed_text, value_color: Theme::TEXT, fade: fade_factor });

    // Remaining (ETA)
    let eta_text = if dl.speed > 0.0 {
        if let Some(total) = dl.total_bytes {
            if total > dl.downloaded_bytes {
                let remaining = total - dl.downloaded_bytes;
                let eta_secs = (remaining as f64 / dl.speed) as u64;
                if eta_secs >= 3600 { format!("{}h {}m {}s", eta_secs / 3600, (eta_secs % 3600) / 60, eta_secs % 60) }
                else if eta_secs >= 60 { format!("{}m {}s", eta_secs / 60, eta_secs % 60) }
                else { format!("{}s", eta_secs) }
            } else { "—".to_string() }
        } else { "Size unknown".to_string() }
    } else { "—".to_string() };
    render_detail_line(frame, Rect { x: inner.x, y: info_start + 7, width: inner.width, height: 1 }, InfoLineParams { icon: "⏳", icon_color: Theme::SKY, label: "Remaining", value: &eta_text, value_color: Theme::TEXT, fade: fade_factor });

    // Second Divider
    let divider2 = Line::from(vec![Span::styled("  ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄", Style::default().fg(fade_color(Theme::BORDER, fade_factor)).add_modifier(Modifier::DIM))]);
    frame.render_widget(Paragraph::new(divider2), Rect { x: inner.x, y: info_start + 8, width: inner.width, height: 1 });

    // Save path
    let path_display = dl.save_path.display().to_string();
    let max_path_len = inner.width.saturating_sub(8) as usize;
    let display_path = if path_display.len() > max_path_len { format!("…{}", &path_display[path_display.len() - max_path_len + 1..]) } else { path_display };
    render_detail_line(frame, Rect { x: inner.x, y: info_start + 9, width: inner.width, height: 1 }, InfoLineParams { icon: "💾", icon_color: Theme::PINK, label: "Path", value: &display_path, value_color: Theme::TEXT_DIM, fade: fade_factor });

    // Verify status
    let (verify_text, verify_color, verify_icon) = match &dl.verify_status {
        VerifyStatus::NotChecked => ("Not checked", Theme::TEXT_DIM, "🔍"),
        VerifyStatus::Pass => ("Checksum OK ✓", Theme::GREEN, "✅"),
        VerifyStatus::Fail => ("Checksum FAILED ✗", Theme::RED, "❌"),
        VerifyStatus::NoHash => ("No hash provided", Theme::TEXT_DIM, "🔍"),
    };
    render_detail_line(frame, Rect { x: inner.x, y: info_start + 10, width: inner.width, height: 1 }, InfoLineParams { icon: verify_icon, icon_color: verify_color, label: "Verify", value: verify_text, value_color: verify_color, fade: fade_factor });

    // Quick Actions
    let actions_y = info_start + 12;
    if actions_y >= inner.y + inner.height { return; }

    let actions = match &dl.status {
        DownloadStatus::Pending => vec![
            Span::styled(" [s] Start ", Style::default().fg(fade_color(Theme::GREEN, fade_factor)).add_modifier(Modifier::BOLD)),
            Span::styled("  ", Style::default()),
            Span::styled(" [d] Delete ", Style::default().fg(fade_color(Theme::RED, fade_factor))),
        ],
        DownloadStatus::Downloading => vec![
            Span::styled(" [p] Pause ", Style::default().fg(fade_color(Theme::YELLOW, fade_factor)).add_modifier(Modifier::BOLD)),
            Span::styled("  ", Style::default()),
            Span::styled(" [d] Delete ", Style::default().fg(fade_color(Theme::RED, fade_factor))),
        ],
        DownloadStatus::Paused => vec![
            Span::styled(" [r] Resume ", Style::default().fg(fade_color(Theme::GREEN, fade_factor)).add_modifier(Modifier::BOLD)),
            Span::styled("  ", Style::default()),
            Span::styled(" [d] Delete ", Style::default().fg(fade_color(Theme::RED, fade_factor))),
        ],
        DownloadStatus::Completed => vec![
            Span::styled(" [v] Verify ", Style::default().fg(fade_color(Theme::TEAL, fade_factor)).add_modifier(Modifier::BOLD)),
            Span::styled("  ", Style::default()),
            Span::styled(" [d] Delete ", Style::default().fg(fade_color(Theme::RED, fade_factor))),
            Span::styled("  ", Style::default()),
            Span::styled(" [R] Rename ", Style::default().fg(fade_color(Theme::TEAL, fade_factor))),
        ],
        DownloadStatus::Failed(_) => vec![
            Span::styled(" [t] Retry ", Style::default().fg(fade_color(Theme::ORANGE, fade_factor)).add_modifier(Modifier::BOLD)),
            Span::styled("  ", Style::default()),
            Span::styled(" [d] Delete ", Style::default().fg(fade_color(Theme::RED, fade_factor))),
        ],
    };
    frame.render_widget(Paragraph::new(Line::from(actions)), Rect { x: inner.x, y: actions_y, width: inner.width, height: 1 });
}
