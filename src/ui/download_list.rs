use ratatui::prelude::*;
use ratatui::widgets::*;
use unicode_width::UnicodeWidthStr;

use super::theme::Theme;
use super::utils::{color_to_rgb, fade_color, format_bytes, format_speed, lerp_channel, truncate_end};
use crate::app::{App, DownloadItem, DownloadStatus};

pub fn draw_download_list(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" 📥 Downloads ")
        .title_style(
            Style::default()
                .fg(Theme::BLUE)
                .add_modifier(Modifier::BOLD),
        )
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::BORDER))
        .style(Style::default().bg(Theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.downloads.is_empty() {
        draw_empty_state(frame, inner);
        return;
    }

    let items: Vec<ListItem> = app
        .downloads
        .iter()
        .enumerate()
        .map(|(i, dl)| dl.to_list_item(i == app.selected, inner.width))
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(Theme::BG_CARD)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" ");

    let mut state = ListState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(list, inner, &mut state);
}

fn draw_empty_state(frame: &mut Frame, area: Rect) {
    let empty_msg = vec![
        Line::from(""),
        Line::from(""),
        Line::from(vec![
            Span::styled("🚀", Style::default().fg(Theme::BLUE)),
            Span::styled("  Ready to download?", Style::default().fg(Theme::BORDER)),
        ]),
        Line::from(vec![
            Span::styled(" Press ", Style::default().fg(Theme::TEXT_DIM)),
            Span::styled(
                "a",
                Style::default()
                    .fg(Theme::GREEN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  to add a download ", Style::default().fg(Theme::TEXT_DIM)),
        ]),
        Line::from(vec![
            Span::styled("or", Style::default().fg(Theme::TEXT_DIM)),
            Span::styled(
                " Ctrl+V ",
                Style::default()
                    .fg(Theme::TEAL)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "to paste from clipboard",
                Style::default().fg(Theme::TEXT_DIM),
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(empty_msg).alignment(Alignment::Center), area);
}

impl DownloadItem {
    pub(super) fn to_list_item(&self, is_selected: bool, width: u16) -> ListItem<'_> {
        let bg = if is_selected {
            Theme::BG_CARD
        } else {
            Theme::BG
        };
        let selected_bg = if is_selected { Theme::BG_HIGHLIGHT } else { bg };

        // Fade-in animation
        let age_ms = self.created_at.elapsed().as_millis() as f64;
        let fade_duration_ms = 1500.0;
        let fade = if age_ms < fade_duration_ms {
            let t = age_ms / fade_duration_ms;
            1.0 - (1.0 - t).powi(3)
        } else {
            1.0
        };

        // Status icon
        let (status_icon, status_color) = match &self.status {
            DownloadStatus::Pending => (" ◷ ", fade_color(Theme::TEXT_DIM, fade)),
            DownloadStatus::Downloading => (" ↓ ", fade_color(Theme::BLUE, fade)),
            DownloadStatus::Paused => (" ❙❙ ", fade_color(Theme::YELLOW, fade)),
            DownloadStatus::Completed => (" ✓ ", fade_color(Theme::GREEN, fade)),
            DownloadStatus::Failed(_) => (" ✗ ", fade_color(Theme::RED, fade)),
        };

        // File name
        let max_name_len = (width as usize).saturating_sub(32).max(12);
        let display_name = truncate_end(&self.filename, max_name_len);
        let name_len = UnicodeWidthStr::width(display_name.as_str());

        // Right-aligned info
        let mut right_spans: Vec<Span> = Vec::new();
        let right_len: usize;

        match &self.status {
            DownloadStatus::Downloading => {
                let speed_str = format_speed(self.speed);
                let progress_str = if self.has_total() {
                    format!("{:.0}%", self.progress())
                } else {
                    format!("{} (size unknown)", self.format_downloaded())
                };
                let elapsed_ms = self
                    .start_time
                    .map(|s| s.elapsed().as_millis())
                    .unwrap_or(0);
                let pulse_cycle = 1800.0;
                let t = (elapsed_ms as f64 % pulse_cycle) / pulse_cycle;
                let pulse = (t * std::f64::consts::TAU).sin() * 0.225 + 0.775;
                let (br, bg_c, bb) = color_to_rgb(Theme::TEAL);
                let r = lerp_channel(br as f64, 235.0, pulse * 0.5);
                let g = lerp_channel(bg_c as f64, 245.0, pulse * 0.5);
                let b = lerp_channel(bb as f64, 255.0, pulse * 0.5);
                let speed_color = Color::Rgb(r as u8, g as u8, b as u8);
                let full = format!("  {} {} ", speed_str, progress_str);
                right_len = full.len();
                right_spans.push(Span::styled(
                    "  ",
                    Style::default().fg(fade_color(Theme::TEAL, fade)),
                ));
                right_spans.push(Span::styled(
                    speed_str.clone(),
                    Style::default()
                        .fg(fade_color(speed_color, fade))
                        .add_modifier(Modifier::BOLD),
                ));
                right_spans.push(Span::styled(
                    format!(" {} ", progress_str),
                    Style::default().fg(fade_color(Theme::TEAL, fade)),
                ));
            }
            DownloadStatus::Completed => {
                right_len = 6;
                right_spans.push(Span::styled(
                    " done ",
                    Style::default()
                        .fg(fade_color(Theme::GREEN, fade))
                        .add_modifier(Modifier::BOLD),
                ));
            }
            DownloadStatus::Paused => {
                right_len = 8;
                right_spans.push(Span::styled(
                    " paused ",
                    Style::default().fg(fade_color(Theme::YELLOW, fade)),
                ));
            }
            DownloadStatus::Failed(_) => {
                right_len = 7;
                right_spans.push(Span::styled(
                    " error ",
                    Style::default().fg(fade_color(Theme::RED, fade)),
                ));
            }
            DownloadStatus::Pending => {
                right_len = 8;
                right_spans.push(Span::styled(
                    " queued ",
                    Style::default().fg(fade_color(Theme::TEXT_DIM, fade)),
                ));
            }
        };

        let padding = (width as usize)
            .saturating_sub(name_len + 12 + right_len)
            .max(2);

        let mut spans = Vec::new();

        // Pulsing glow ring
        if is_selected {
            let elapsed_ms = self
                .start_time
                .map(|s| s.elapsed().as_millis())
                .unwrap_or(self.elapsed.as_millis());
            let glow_cycle = 2000.0;
            let t = (elapsed_ms as f64 % glow_cycle) / glow_cycle;
            let pulse = (t * std::f64::consts::TAU).sin() * 0.3 + 0.7;
            let r = lerp_channel(120.0, 255.0, pulse * 0.55);
            let g = lerp_channel(130.0, 240.0, pulse * 0.55);
            let b = lerp_channel(200.0, 255.0, pulse * 0.55);
            let glow_color = fade_color(Color::Rgb(r as u8, g as u8, b as u8), fade);
            spans.push(Span::styled(
                "▌",
                Style::default().fg(glow_color).add_modifier(Modifier::BOLD),
            ));
        }

        spans.push(Span::styled(status_icon, Style::default().fg(status_color)));
        spans.push(Span::styled(
            display_name,
            Style::default().fg(fade_color(
                if is_selected {
                    Theme::TEXT_BRIGHT
                } else {
                    Theme::TEXT
                },
                fade,
            )),
        ));
        spans.push(Span::styled(
            " ".repeat(padding),
            Style::default().fg(fade_color(Theme::TEXT_DIM, fade)),
        ));
        spans.extend(right_spans);

        let line = Line::from(spans);
        let mut lines = vec![line];

        // Progress bar
        if matches!(
            self.status,
            DownloadStatus::Downloading | DownloadStatus::Paused
        ) {
            let bar_width = (width as usize).saturating_sub(8).min(60);
            if bar_width > 5 {
                let bar_color = match &self.status {
                    DownloadStatus::Downloading => Theme::BLUE,
                    DownloadStatus::Paused => Theme::YELLOW,
                    _ => Theme::TEXT_DIM,
                };

                let bar_spans = if self.has_total() {
                    // Known total: shimmer progress bar
                    let filled_pct = self.progress() / 100.0;
                    let filled = (bar_width as f64 * filled_pct) as usize;
                    let label = format!(" {:.1}%", self.progress());
                    let elapsed_ms = self
                        .start_time
                        .map(|s| s.elapsed().as_millis())
                        .unwrap_or(self.elapsed.as_millis());
                    let shimmer_cycle = 2000;
                    let shimmer_width = (bar_width / 5).max(3);
                    let phase = (elapsed_ms % shimmer_cycle) as f64 / shimmer_cycle as f64;
                    let shimmer_center = phase * bar_width as f64;

                    let mut bar_spans: Vec<Span> =
                        vec![Span::styled("    ", Style::default().bg(bg))];
                    for i in 0..bar_width {
                        if i < filled {
                            let dist = (i as f64 - shimmer_center).abs();
                            let dist_wrapped = dist.min(bar_width as f64 - dist);
                            let half_shimmer = shimmer_width as f64 / 2.0;
                            let char_color = if dist_wrapped < half_shimmer {
                                let intensity = 1.0 - (dist_wrapped / half_shimmer);
                                let (br, bg_c, bb) = color_to_rgb(bar_color);
                                let r = lerp_channel(br as f64, 245.0, intensity * 0.6);
                                let g = lerp_channel(bg_c as f64, 224.0, intensity * 0.6);
                                let b = lerp_channel(bb as f64, 255.0, intensity * 0.6);
                                Color::Rgb(r as u8, g as u8, b as u8)
                            } else {
                                bar_color
                            };
                            bar_spans
                                .push(Span::styled("█", Style::default().fg(char_color).bg(bg)));
                        } else {
                            bar_spans.push(Span::styled(
                                "░",
                                Style::default().fg(Theme::PROGRESS_BG).bg(bg),
                            ));
                        }
                    }
                    bar_spans.push(Span::styled(
                        label,
                        Style::default()
                            .fg(bar_color)
                            .add_modifier(Modifier::BOLD)
                            .bg(bg),
                    ));
                    bar_spans
                } else {
                    // Indeterminate: animated snake with shimmer
                    let elapsed_ms = self
                        .start_time
                        .map(|s| s.elapsed().as_millis())
                        .unwrap_or(self.elapsed.as_millis());
                    let cycle = 1500;
                    let snake_len = (bar_width / 3).max(4);
                    let phase = (elapsed_ms % cycle) as f64 / cycle as f64;
                    let snake_head = phase * bar_width as f64;

                    let mut bar_spans: Vec<Span> =
                        vec![Span::styled("    ", Style::default().bg(bg))];
                    for i in 0..bar_width {
                        let dist = (i as f64 - snake_head).abs();
                        let dist_wrapped = dist.min(bar_width as f64 - dist);
                        if dist_wrapped < snake_len as f64 / 2.0 {
                            let t = 1.0 - (dist_wrapped / (snake_len as f64 / 2.0));
                            let head_bonus = if dist_wrapped < 1.0 { 0.3 } else { 0.0 };
                            let intensity = (t * 0.7 + head_bonus).min(1.0);
                            let (br, bg_c, bb) = color_to_rgb(bar_color);
                            let r = lerp_channel(br as f64, 255.0, intensity * 0.5);
                            let g = lerp_channel(bg_c as f64, 240.0, intensity * 0.5);
                            let b = lerp_channel(bb as f64, 255.0, intensity * 0.5);
                            let snake_char = if dist_wrapped < 1.0 { "▓" } else { "█" };
                            bar_spans.push(Span::styled(
                                snake_char,
                                Style::default()
                                    .fg(Color::Rgb(r as u8, g as u8, b as u8))
                                    .bg(bg),
                            ));
                        } else {
                            bar_spans.push(Span::styled(
                                "░",
                                Style::default().fg(Theme::PROGRESS_BG).bg(bg),
                            ));
                        }
                    }
                    let label = format!(" {}", self.format_downloaded());
                    bar_spans.push(Span::styled(
                        label,
                        Style::default()
                            .fg(bar_color)
                            .add_modifier(Modifier::ITALIC)
                            .bg(bg),
                    ));
                    bar_spans
                };

                lines.push(Line::from(bar_spans));

                // Chunk indicators
                if self.num_chunks > 1 {
                    let elapsed_ms = self
                        .start_time
                        .map(|s| s.elapsed().as_millis())
                        .unwrap_or(self.elapsed.as_millis());
                    let mut chunk_spans = vec![Span::styled("    ", Style::default().bg(bg))];
                    for ci in 0..self.num_chunks {
                        let cp = self.chunk_progress(ci);
                        let (chunk_char, chunk_color) = if cp >= 100.0 {
                            ("◆", Theme::GREEN)
                        } else if cp > 0.0 {
                            let phase_offset = ci as f64 * 0.8;
                            let pulse_cycle = 1200.0;
                            let t = ((elapsed_ms as f64 + phase_offset * 1000.0) % pulse_cycle)
                                / pulse_cycle;
                            let pulse = (t * std::f64::consts::TAU).sin() * 0.35 + 0.65;
                            let r = lerp_channel(40.0, 148.0, pulse);
                            let g = lerp_channel(80.0, 226.0, pulse);
                            let b = lerp_channel(180.0, 255.0, pulse);
                            ("◆", Color::Rgb(r as u8, g as u8, b as u8))
                        } else {
                            let idle_cycle = 3000.0;
                            let t =
                                ((elapsed_ms as f64 + ci as f64 * 600.0) % idle_cycle) / idle_cycle;
                            let pulse = (t * std::f64::consts::TAU).sin() * 0.15 + 0.35;
                            let c = lerp_channel(69.0, 108.0, pulse);
                            ("○", Color::Rgb(c as u8, c as u8, c as u8))
                        };
                        chunk_spans.push(Span::styled(
                            chunk_char,
                            Style::default().fg(chunk_color).bg(bg),
                        ));
                    }
                    chunk_spans.push(Span::styled(
                        format!("  {} chunks", self.num_chunks),
                        Style::default()
                            .fg(Theme::TEXT_DIM)
                            .add_modifier(Modifier::ITALIC)
                            .bg(bg),
                    ));
                    lines.push(Line::from(chunk_spans));
                }
            }
        } else if self.status == DownloadStatus::Pending {
            lines.push(Line::from(vec![Span::styled(
                "    ◷  Waiting for slot…",
                Style::default()
                    .fg(Theme::TEXT_DIM)
                    .add_modifier(Modifier::ITALIC),
            )]));
        } else if self.status == DownloadStatus::Completed {
            let size_str = format_bytes(self.downloaded_bytes);
            lines.push(Line::from(vec![
                Span::styled("      ", Style::default()),
                Span::styled(
                    format!("{} ({})", self.save_path.display(), size_str),
                    Style::default()
                        .fg(Theme::TEXT_DIM)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
        } else if let DownloadStatus::Failed(ref err) = self.status {
            let max_err_len = (width as usize).saturating_sub(10);
            let display_err = truncate_end(err, max_err_len);
            lines.push(Line::from(vec![
                Span::styled("    ✗  ", Style::default().fg(Theme::RED)),
                Span::styled(
                    format!("Error: {}", display_err),
                    Style::default().fg(Theme::RED),
                ),
            ]));
        }

        ListItem::new(lines).style(Style::default().bg(selected_bg))
    }
}
