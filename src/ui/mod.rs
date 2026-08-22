pub mod theme;
pub mod utils;
pub mod header;
pub mod download_list;
pub mod detail_panel;
pub mod footer;
pub mod modals;

use ratatui::prelude::*;

use crate::app::{App, InputMode};

use self::header::draw_header;
use self::download_list::draw_download_list;
use self::detail_panel::draw_detail_panel;
use self::footer::draw_footer;
use self::modals::*;

/// Render the full application UI
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Main layout: vertical split
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Header
            Constraint::Min(10),  // Content
            Constraint::Length(3), // Footer
        ])
        .split(area);

    draw_header(frame, main_layout[0]);

    // Content area: split into download list + info panel
    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(62), // Download list
            Constraint::Percentage(38), // Info / details panel
        ])
        .split(main_layout[1]);

    draw_download_list(frame, content_layout[0], app);
    draw_detail_panel(frame, content_layout[1], app);

    draw_footer(frame, main_layout[2], app);

    // Overlays
    match app.input_mode {
        InputMode::AddingUrl => draw_input_modal(frame, area, "Enter URL", &app.input_buf),
        InputMode::AddingName => draw_input_modal(frame, area, "Enter filename", &app.input_buf),
        InputMode::AddingSha256 => draw_input_modal(frame, area, "SHA-256 hash or Enter to skip", &app.input_buf),
        InputMode::Previewing => draw_preview_modal(frame, area, app),
        InputMode::RenameFile => draw_input_modal(frame, area, "Rename file", &app.input_buf),
        InputMode::ConfirmDelete => draw_confirm_modal(frame, area, app),
        InputMode::Help => draw_help_overlay(frame, area),
        InputMode::DirectoryBrowser => draw_directory_browser(frame, area, app),
        InputMode::SettingSpeedLimit => draw_input_modal(frame, area, "Speed limit (e.g. 500k, 10m, 0=unlimited)", &app.input_buf),
        InputMode::SettingConcurrency => draw_input_modal(frame, area, "Max concurrent downloads (1-10)", &app.input_buf),
        InputMode::StatsDashboard => draw_stats_dashboard(frame, area, app),
        InputMode::Settings => draw_settings_modal(frame, area, app),
        InputMode::SettingField(_) => draw_settings_modal(frame, area, app),
        InputMode::Normal => {}
    }
}
