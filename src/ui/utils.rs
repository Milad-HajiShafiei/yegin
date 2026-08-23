use ratatui::prelude::*;
use ratatui::widgets::*;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::theme::Theme;

/// Fade a color by a factor (0.0 = fully black/invisible, 1.0 = original color).
pub fn fade_color(c: Color, factor: f64) -> Color {
    let f = factor.clamp(0.0, 1.0);
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f64 * f) as u8,
            (g as f64 * f) as u8,
            (b as f64 * f) as u8,
        ),
        other => other,
    }
}

/// Create a centered rect using percent of total area
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Create a centered popup that keeps enough room for its content whenever
/// the terminal can provide it.
pub fn centered_rect_with_min(
    percent_x: u16,
    percent_y: u16,
    min_width: u16,
    min_height: u16,
    area: Rect,
) -> Rect {
    let width = ((area.width as u32 * percent_x as u32 / 100) as u16)
        .max(min_width)
        .min(area.width);
    let height = ((area.height as u32 * percent_y as u32 / 100) as u16)
        .max(min_height)
        .min(area.height);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

/// Truncate a string to terminal cell width without splitting UTF-8.
pub fn truncate_end(value: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(value) <= max_width {
        return value.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }

    let mut result = String::new();
    let mut width = 0;
    for character in value.chars() {
        let char_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if width + char_width > max_width - 1 {
            break;
        }
        result.push(character);
        width += char_width;
    }
    result.push('…');
    result
}

/// Truncate from the beginning, preserving the filename/path suffix.
pub fn truncate_start(value: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(value) <= max_width {
        return value.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }

    let mut suffix = String::new();
    let mut width = 0;
    for character in value.chars().rev() {
        let char_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if width + char_width > max_width - 1 {
            break;
        }
        suffix.insert(0, character);
        width += char_width;
    }
    format!("…{suffix}")
}

/// Linearly interpolate a single color channel between two values.
/// `t` should be in range [0.0, 1.0].
pub fn lerp_channel(a: f64, b: f64, t: f64) -> f64 {
    (a + (b - a) * t.clamp(0.0, 1.0)).round().clamp(0.0, 255.0)
}

/// Extract RGB components from a `ratatui::Color`. Falls back to gray if not Rgb.
pub fn color_to_rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (128, 128, 128),
    }
}

pub fn format_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec < 1024.0 {
        format!("{:.0} B", bytes_per_sec)
    } else if bytes_per_sec < 1024.0 * 1024.0 {
        format!("{:.1} KB", bytes_per_sec / 1024.0)
    } else if bytes_per_sec < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", bytes_per_sec / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes_per_sec / (1024.0 * 1024.0 * 1024.0))
    }
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Visual parameters for rendering a consistent info line.
pub struct InfoLineParams<'a> {
    pub icon: &'a str,
    pub icon_color: Color,
    pub label: &'a str,
    pub value: &'a str,
    pub value_color: Color,
    pub fade: f64,
}

/// Helper to render a consistent detail line
pub fn render_detail_line(frame: &mut Frame, area: Rect, p: InfoLineParams<'_>) {
    let line = Line::from(vec![
        Span::styled(
            format!("  {} ", p.icon),
            Style::default().fg(fade_color(p.icon_color, p.fade)),
        ),
        Span::styled(
            format!("{}: ", p.label),
            Style::default()
                .fg(fade_color(Theme::TEXT_DIM, p.fade))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(p.value, Style::default().fg(fade_color(p.value_color, p.fade))),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── format_speed ─────────────────────────────────────────────────────

    #[test]
    fn format_speed_zero() {
        assert_eq!(format_speed(0.0), "0 B");
    }

    #[test]
    fn format_speed_bytes() {
        assert_eq!(format_speed(512.0), "512 B");
        assert_eq!(format_speed(1023.9), "1024 B");
    }

    #[test]
    fn format_speed_kilobytes() {
        assert_eq!(format_speed(1024.0), "1.0 KB");
        assert_eq!(format_speed(1536.0), "1.5 KB");
        assert_eq!(format_speed(1024.0 * 1023.0), "1023.0 KB");
    }

    #[test]
    fn format_speed_megabytes() {
        assert_eq!(format_speed(1024.0 * 1024.0), "1.0 MB");
        assert_eq!(format_speed(1024.0 * 1024.0 * 5.5), "5.5 MB");
    }

    #[test]
    fn format_speed_gigabytes() {
        assert_eq!(format_speed(1024.0 * 1024.0 * 1024.0), "1.00 GB");
        assert_eq!(format_speed(1024.0 * 1024.0 * 1024.0 * 2.5), "2.50 GB");
    }

    // ── format_bytes ─────────────────────────────────────────────────────

    #[test]
    fn format_bytes_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn format_bytes_small() {
        assert_eq!(format_bytes(42), "42 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn format_bytes_kilobytes() {
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
    }

    #[test]
    fn format_bytes_megabytes() {
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 50), "50.0 MB");
    }

    #[test]
    fn format_bytes_gigabytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 10), "10.00 GB");
    }

    // ── lerp_channel ────────────────────────────────────────────────────

    #[test]
    fn lerp_channel_at_zero() {
        assert_eq!(lerp_channel(0.0, 255.0, 0.0), 0.0);
    }

    #[test]
    fn lerp_channel_at_one() {
        assert_eq!(lerp_channel(0.0, 255.0, 1.0), 255.0);
    }

    #[test]
    fn lerp_channel_midpoint() {
        let result = lerp_channel(0.0, 200.0, 0.5);
        assert_eq!(result, 100.0);
    }

    #[test]
    fn lerp_channel_clamped_below() {
        assert_eq!(lerp_channel(0.0, 255.0, -0.5), 0.0);
    }

    #[test]
    fn lerp_channel_clamped_above() {
        assert_eq!(lerp_channel(0.0, 255.0, 1.5), 255.0);
    }

    #[test]
    fn lerp_channel_clamped_at_255() {
        let result = lerp_channel(200.0, 300.0, 1.0);
        assert_eq!(result, 255.0);
    }

    #[test]
    fn lerp_channel_clamped_at_0() {
        let result = lerp_channel(50.0, 0.0, 1.0);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn lerp_channel_no_change() {
        assert_eq!(lerp_channel(100.0, 100.0, 0.5), 100.0);
    }

    // ── color_to_rgb ─────────────────────────────────────────────────────

    #[test]
    fn color_to_rgb_extract() {
        let c = Color::Rgb(42, 128, 200);
        assert_eq!(color_to_rgb(c), (42, 128, 200));
    }

    #[test]
    fn color_to_rgb_non_rgb_fallback() {
        assert_eq!(color_to_rgb(Color::Red), (128, 128, 128));
        assert_eq!(color_to_rgb(Color::Blue), (128, 128, 128));
        assert_eq!(color_to_rgb(Color::Reset), (128, 128, 128));
    }

    #[test]
    fn color_to_rgb_black() {
        assert_eq!(color_to_rgb(Color::Rgb(0, 0, 0)), (0, 0, 0));
    }

    #[test]
    fn color_to_rgb_white() {
        assert_eq!(color_to_rgb(Color::Rgb(255, 255, 255)), (255, 255, 255));
    }

    // ── fade_color ───────────────────────────────────────────────────────

    #[test]
    fn fade_color_full() {
        let c = Color::Rgb(100, 150, 200);
        assert_eq!(fade_color(c, 1.0), Color::Rgb(100, 150, 200));
    }

    #[test]
    fn fade_color_zero() {
        let c = Color::Rgb(100, 150, 200);
        assert_eq!(fade_color(c, 0.0), Color::Rgb(0, 0, 0));
    }

    #[test]
    fn fade_color_half() {
        let c = Color::Rgb(100, 200, 100);
        let result = fade_color(c, 0.5);
        assert_eq!(result, Color::Rgb(50, 100, 50));
    }

    #[test]
    fn fade_color_clamped_below() {
        let c = Color::Rgb(100, 100, 100);
        assert_eq!(fade_color(c, -0.5), Color::Rgb(0, 0, 0));
    }

    #[test]
    fn fade_color_clamped_above() {
        let c = Color::Rgb(100, 100, 100);
        assert_eq!(fade_color(c, 1.5), Color::Rgb(100, 100, 100));
    }

    #[test]
    fn fade_color_non_rgb_passthrough() {
        let c = Color::Red;
        assert_eq!(fade_color(c, 0.5), Color::Red);
    }

    // ── centered_rect ────────────────────────────────────────────────────

    #[test]
    fn centered_rect_full_size() {
        let area = Rect::new(0, 0, 100, 50);
        let result = centered_rect(100, 100, area);
        assert_eq!(result, area);
    }

    #[test]
    fn centered_rect_half_width() {
        let area = Rect::new(0, 0, 100, 50);
        let result = centered_rect(50, 100, area);
        assert_eq!(result.width, 50);
        assert_eq!(result.x, 25);
    }

    #[test]
    fn centered_rect_half_height() {
        let area = Rect::new(0, 0, 100, 50);
        let result = centered_rect(100, 50, area);
        assert_eq!(result.height, 25);
        assert!(result.y == 12 || result.y == 13, "y should be 12 or 13, got {}", result.y);
    }

    #[test]
    fn centered_rect_small_popup() {
        let area = Rect::new(0, 0, 120, 40);
        let result = centered_rect(30, 20, area);
        assert_eq!(result.width, 36);
        assert_eq!(result.height, 8);
    }

    #[test]
    fn centered_rect_zero_percent() {
        let area = Rect::new(0, 0, 100, 50);
        let result = centered_rect(0, 0, area);
        assert_eq!(result.width, 0);
        assert_eq!(result.height, 0);
    }

    #[test]
    fn centered_rect_preserves_position() {
        let area = Rect::new(10, 5, 80, 30);
        let result = centered_rect(50, 50, area);
        assert!(result.x >= 10);
        assert!(result.y >= 5);
        assert!(result.x + result.width <= 90);
        assert!(result.y + result.height <= 35);
    }

    #[test]
    fn centered_rect_with_min_fits_normal_terminal_content() {
        let result = centered_rect_with_min(45, 16, 48, 7, Rect::new(0, 0, 80, 24));
        assert_eq!(result.width, 48);
        assert_eq!(result.height, 7);
        assert_eq!(result.x, 16);
        assert_eq!(result.y, 8);
    }

    #[test]
    fn centered_rect_with_min_never_exceeds_tiny_terminal() {
        let area = Rect::new(0, 0, 8, 3);
        assert_eq!(centered_rect_with_min(45, 16, 48, 7, area), area);
    }

    #[test]
    fn truncation_is_utf8_safe_and_width_bounded() {
        let value = "گزارش-🚀-دانلود";
        let end = truncate_end(value, 7);
        let start = truncate_start(value, 7);
        assert!(UnicodeWidthStr::width(end.as_str()) <= 7);
        assert!(UnicodeWidthStr::width(start.as_str()) <= 7);
        assert!(end.ends_with('…'));
        assert!(start.starts_with('…'));
    }

    #[test]
    fn truncation_handles_zero_width_without_panicking() {
        assert_eq!(truncate_end("گزارش", 0), "");
        assert_eq!(truncate_start("گزارش", 0), "");
        assert_eq!(truncate_end("گزارش", 1), "…");
        assert_eq!(truncate_start("گزارش", 1), "…");
    }

    // ── format_speed edge cases ──────────────────────────────────────────

    #[test]
    fn format_speed_negative() {
        // Negative speeds should still format (no panic)
        let result = format_speed(-1.0);
        assert!(result.contains("B"));
    }

    #[test]
    fn format_speed_very_large() {
        let tb = 1024.0 * 1024.0 * 1024.0 * 1024.0;
        let result = format_speed(tb);
        assert!(result.contains("GB") || result.contains("TB"));
    }

    // ── format_bytes edge cases ─────────────────────────────────────────

    #[test]
    fn format_bytes_exact_kb_boundary() {
        assert_eq!(format_bytes(1024), "1.0 KB");
    }

    #[test]
    fn format_bytes_exact_mb_boundary() {
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn format_bytes_exact_gb_boundary() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn format_bytes_large_mb() {
        assert_eq!(format_bytes(1024 * 1024 * 999), "999.0 MB");
    }

    // ── fade_color edge cases ───────────────────────────────────────────

    #[test]
    fn fade_color_near_zero() {
        let c = Color::Rgb(200, 200, 200);
        let result = fade_color(c, 0.01);
        assert_eq!(result, Color::Rgb(2, 2, 2));
    }

    #[test]
    fn fade_color_near_one() {
        let c = Color::Rgb(200, 200, 200);
        let result = fade_color(c, 0.99);
        assert_eq!(result, Color::Rgb(198, 198, 198));
    }

    // ── color_to_rgb edge cases ─────────────────────────────────────────

    #[test]
    fn color_to_rgb_indexed_color() {
        // Indexed colors fall back to gray
        assert_eq!(color_to_rgb(Color::Indexed(1)), (128, 128, 128));
    }

    // ── lerp_channel edge cases ─────────────────────────────────────────

    #[test]
    fn lerp_channel_same_values() {
        assert_eq!(lerp_channel(128.0, 128.0, 0.0), 128.0);
        assert_eq!(lerp_channel(128.0, 128.0, 0.5), 128.0);
        assert_eq!(lerp_channel(128.0, 128.0, 1.0), 128.0);
    }

    #[test]
    fn lerp_channel_reversed() {
        // a > b: interpolating downward
        let result = lerp_channel(200.0, 100.0, 0.5);
        assert_eq!(result, 150.0);
    }

    #[test]
    fn lerp_channel_quarter() {
        let result = lerp_channel(0.0, 200.0, 0.25);
        assert_eq!(result, 50.0);
    }

    #[test]
    fn lerp_channel_three_quarters() {
        let result = lerp_channel(0.0, 200.0, 0.75);
        assert_eq!(result, 150.0);
    }
}
