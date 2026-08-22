mod app;
mod ui;

use app::{App, InputMode, DownloadStatus, fetch_head_info, NUM_SETTINGS};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut app = App::new();
    let tick_rate = Duration::from_millis(50);
    let mut last_tick = Instant::now();

    loop {
        // Poll for events
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                handle_key_event(&mut app, key);
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.poll_events();
            app.auto_save();
            // Poll for HEAD request preview result
            if app.input_mode == InputMode::Previewing {
                if let Some(rx) = &mut app.pending_preview_rx {
                    if let Ok(preview) = rx.try_recv() {
                        // Extract filename for display
                        let mut p = preview;
                        if p.filename.is_empty() {
                            p.filename = app.filename_from_url(&p.url);
                        }
                        app.preview = Some(p);
                        app.pending_preview_rx = None;
                        app.preview_ready = true;
                    }
                }
            }
            last_tick = Instant::now();
        }

        // Render
        terminal.draw(|frame| {
            ui::draw(frame, &app);
        })?;

        if !app.running {
            break;
        }
    }

    // Set cancel token on all active downloads so chunk tasks flush and exit
    for item in &app.downloads {
        if item.status == DownloadStatus::Downloading {
            item.cancel_token.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
    // Give chunk tasks time to flush and drain all events so chunk_downloaded
    // is up-to-date before saving state.
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.poll_events();
    }

    // Save all state before exiting so downloads can be resumed next time
    app.save_state();
    app.save_history();
    app.save_settings();

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn handle_key_event(app: &mut App, key: KeyEvent) {
    // Global keys (work in any mode)
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.running = false;
            return;
        }
        KeyCode::Esc => {
            match app.input_mode {
                InputMode::AddingUrl | InputMode::AddingName | InputMode::AddingSha256 | InputMode::RenameFile => {
                    app.input_mode = InputMode::Normal;
                    app.input_buf.clear();
                    app.pending_url = None;
                    app.pending_filename = None;
                }
                InputMode::Previewing => {
                    app.input_mode = InputMode::Normal;
                    app.preview = None;
                    app.pending_url = None;
                    app.pending_filename = None;
                    app.pending_sha256 = None;
                    app.pending_filename_stored = None;
                    app.pending_preview_rx = None;
                    app.preview_ready = false;
                }
                InputMode::SettingSpeedLimit | InputMode::SettingConcurrency => {
                    app.input_mode = InputMode::Normal;
                    app.input_buf.clear();
                }
                InputMode::ConfirmDelete
                | InputMode::Help
                | InputMode::DirectoryBrowser
                | InputMode::StatsDashboard
                | InputMode::Settings => {
                    app.input_mode = InputMode::Normal;
                }
                InputMode::SettingField(_) => {
                    // Esc in SettingField goes back to Settings (not Normal)
                    app.input_mode = InputMode::Settings;
                    app.input_buf.clear();
                }
                InputMode::Normal => {
                    app.running = false;
                }
            }
            return;
        }
        _ => {}
    }

    // Handle input modes that accept text typing (Enter to confirm, Backspace, Char to type)
    match app.input_mode {
        InputMode::AddingUrl
        | InputMode::AddingName
        | InputMode::AddingSha256
        | InputMode::RenameFile
        | InputMode::SettingSpeedLimit
        | InputMode::SettingConcurrency => {
            handle_text_input(app, key);
            return;
        }
        _ => {}
    }

    // Handle non-typing modes
    match app.input_mode {
        InputMode::SettingField(_) => {
            // SettingField Enter handler is at the bottom of this function
            // But we need text input to work, so handle it here
            handle_text_input(app, key);
        }
        InputMode::ConfirmDelete => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if app.selected < app.downloads.len() {
                    let filename = app.downloads[app.selected].filename.clone();
                    let id = app.downloads[app.selected].id;
                    app.remove_download(id);
                    app.set_status(format!("Deleted \"{}\"", filename));
                }
                app.input_mode = InputMode::Normal;
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                app.input_mode = InputMode::Normal;
            }
            _ => {}
        },
        InputMode::Help => {
            // Any key closes help
            app.input_mode = InputMode::Normal;
        }
        InputMode::DirectoryBrowser => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.browser_select_up();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.browser_select_down();
            }
            KeyCode::Enter => {
                app.browser_enter();
            }
            KeyCode::Backspace => {
                app.browser_go_up();
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                app.browser_select_current();
            }
            KeyCode::PageUp => {
                for _ in 0..5 {
                    app.browser_select_up();
                }
            }
            KeyCode::PageDown => {
                for _ in 0..5 {
                    app.browser_select_down();
                }
            }
            _ => {}
        },
        InputMode::StatsDashboard => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.history_scroll = app.history_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if app.history_scroll + 1 < app.history.len() {
                    app.history_scroll += 1;
                }
            }
            KeyCode::PageUp => {
                app.history_scroll = app.history_scroll.saturating_sub(5);
            }
            KeyCode::PageDown => {
                app.history_scroll = (app.history_scroll + 5).min(app.history.len().saturating_sub(1));
            }
            KeyCode::Home | KeyCode::Char('g') => {
                app.history_scroll = 0;
            }
            KeyCode::End | KeyCode::Char('G') => {
                app.history_scroll = app.history.len().saturating_sub(1);
            }
            _ => {}
        },
        InputMode::Settings => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.settings_selected = app.settings_selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if app.settings_selected + 1 < NUM_SETTINGS {
                    app.settings_selected += 1;
                }
            }
            KeyCode::Enter => {
                app.input_mode = InputMode::SettingField(app.settings_selected);
                app.input_buf.clear();
            }
            _ => {}
        },
        InputMode::Previewing => if key.code == KeyCode::Enter {
            // Only start download if preview has been rendered at least once
            // (set to true by the tick handler after first successful poll)
            if !app.preview_ready {
                return;
            }
            // Eagerly poll preview one more time in case it just arrived
            if app.preview.is_none() {
                if let Some(rx) = &mut app.pending_preview_rx {
                    if let Ok(preview) = rx.try_recv() {
                        let mut p = preview;
                        if p.filename.is_empty() {
                            p.filename = app.filename_from_url(&p.url);
                        }
                        app.preview = Some(p);
                    }
                }
            }
            // User confirmed — start the download
            let url = app.pending_url.take().unwrap();
            let filename = app.pending_filename_stored.take();
            let sha256 = app.pending_sha256.take();
            // User's typed filename takes priority; fall back to HEAD request filename
            let final_filename = filename
                .or_else(|| app.preview.as_ref()
                    .and_then(|p| if p.filename.is_empty() { None } else { Some(p.filename.clone()) }));
            app.preview = None;
            app.pending_preview_rx = None;
            app.preview_ready = false;
            app.add_download(url, final_filename, sha256);
            let id = app.downloads.last().map(|d| d.id).unwrap_or(0);
            app.start_download(id);
            app.input_mode = InputMode::Normal;
            app.input_buf.clear();
        },
        InputMode::Normal => {
            match key.code {
                // Quit
                KeyCode::Char('q') => {
                    app.running = false;
                }

                // Navigation
                KeyCode::Up | KeyCode::Char('k') => {
                    app.move_selection_up();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    app.move_selection_down();
                }
                KeyCode::PageUp => {
                    for _ in 0..10 {
                        app.move_selection_up();
                    }
                }
                KeyCode::PageDown => {
                    for _ in 0..10 {
                        app.move_selection_down();
                    }
                }
                KeyCode::Home | KeyCode::Char('g') => {
                    app.selected = 0;
                }
                KeyCode::End | KeyCode::Char('G') => {
                    if !app.downloads.is_empty() {
                        app.selected = app.downloads.len() - 1;
                    }
                }

                // Actions
                KeyCode::Char('a') => {
                    app.input_mode = InputMode::AddingUrl;
                    app.input_buf.clear();
                }

                KeyCode::Char('c') => {
                    app.open_browser();
                }

                KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.paste_from_clipboard();
                }

                KeyCode::Char('s') => {
                    if let Some(item) = app.downloads.get(app.selected) {
                        let id = item.id;
                        let status = item.status.clone();
                        match status {
                            DownloadStatus::Pending | DownloadStatus::Paused => {
                                app.start_download(id);
                                app.set_status(format!("Starting download #{}", id));
                            }
                            _ => {}
                        }
                    }
                }

                KeyCode::Char('p') => {
                    if let Some(item) = app.downloads.get(app.selected) {
                        if item.status == DownloadStatus::Downloading {
                            let id = item.id;
                            let name = item.filename.clone();
                            app.pause_download(id);
                            app.set_status(format!("Paused \"{}\"", name));
                        }
                    }
                }

                KeyCode::Char('r') => {
                    if let Some(item) = app.downloads.get(app.selected) {
                        if item.status == DownloadStatus::Paused {
                            let id = item.id;
                            let name = item.filename.clone();
                            app.resume_download(id);
                            app.set_status(format!("Resumed \"{}\"", name));
                        }
                    }
                }

                KeyCode::Char('t') => {
                    if let Some(item) = app.downloads.get(app.selected) {
                        if matches!(item.status, DownloadStatus::Failed(_)) {
                            let id = item.id;
                            app.retry_download(id);
                            app.set_status(format!("Retrying download #{}", id));
                        }
                    }
                }

                KeyCode::Char('d') | KeyCode::Delete => {
                    if !app.downloads.is_empty() {
                        app.input_mode = InputMode::ConfirmDelete;
                    }
                }

                KeyCode::Char('x') => {
                    app.clear_completed();
                }

                KeyCode::Char('R') => {
                    if app.selected < app.downloads.len() {
                        app.input_mode = InputMode::RenameFile;
                        app.input_buf.clear();
                    }
                }

                KeyCode::Char('v') => {
                    if app.selected < app.downloads.len() {
                        let id = app.downloads[app.selected].id;
                        app.verify_download(id);
                    }
                }

                KeyCode::Char('L') => {
                    if app.selected < app.downloads.len() {
                        app.input_mode = InputMode::SettingSpeedLimit;
                        app.input_buf.clear();
                    }
                }

                KeyCode::Char('S') => {
                    app.input_mode = InputMode::StatsDashboard;
                }

                KeyCode::Char('M') => {
                    app.input_mode = InputMode::SettingConcurrency;
                    app.input_buf.clear();
                }

                KeyCode::Char('o') => {
                    if app.selected < app.downloads.len() {
                        let id = app.downloads[app.selected].id;
                        app.open_folder(id);
                    }
                }

                KeyCode::Char(':') => {
                    app.input_mode = InputMode::Settings;
                    app.settings_selected = 0;
                }

                // Toggle help
                KeyCode::Char('?') => {
                    app.input_mode = InputMode::Help;
                }

                _ => {}
            }
        }
        _ => {}
    }
}

/// Handle text input for all modes that accept typing (Enter to confirm, Backspace, Char).
fn handle_text_input(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            let value = app.input_buf.trim().to_string();
            match app.input_mode {
                InputMode::AddingUrl => {
                    if value.is_empty() {
                        app.set_status("URL cannot be empty".into());
                        app.input_mode = InputMode::Normal;
                        app.input_buf.clear();
                        return;
                    }

                    // Validate URL
                    if !value.starts_with("http://") && !value.starts_with("https://") {
                        app.set_status("URL must start with http:// or https://".into());
                        app.input_mode = InputMode::Normal;
                        app.input_buf.clear();
                        return;
                    }

                    app.pending_url = Some(value);
                    app.input_buf.clear();
                    app.input_mode = InputMode::AddingName;
                }
                InputMode::AddingName => {
                    // Store filename separately so it doesn't get overwritten by SHA-256 input
                    // If user didn't include an extension, try to get it from the URL
                    let final_name = if value.is_empty() {
                        None
                    } else {
                        let has_ext = Path::new(&value).extension().is_some();
                        if has_ext {
                            Some(value)
                        } else {
                            // Extract extension from the pending URL
                            let ext = app.pending_url.as_ref()
                                .and_then(|u| url::Url::parse(u).ok())
                                .and_then(|u| {
                                    let path = u.path();
                                    Path::new(path).extension()
                                        .map(|e| e.to_string_lossy().to_string())
                                });
                            match ext {
                                Some(e) => Some(format!("{}.{}", value, e)),
                                None => Some(value),
                            }
                        }
                    };
                    app.pending_filename = final_name;
                    app.input_buf.clear();
                    app.input_mode = InputMode::AddingSha256;
                }
                InputMode::AddingSha256 => {
                    let sha256 = if value.is_empty() { None } else { Some(value) };
                    let url = app.pending_url.clone().unwrap();
                    let filename = app.pending_filename.clone();
                    app.pending_sha256 = sha256;
                    app.pending_filename_stored = filename;
                    app.input_buf.clear();
                    app.input_mode = InputMode::Previewing;
                    app.preview_ready = false;
                    // Spawn async HEAD request to fetch preview info
                    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                    let client_url = url.clone();
                    tokio::spawn(async move {
                        let info = fetch_head_info(&client_url).await;
                        let _ = tx.send(info);
                    });
                    app.pending_preview_rx = Some(rx);
                }
                InputMode::RenameFile => {
                    if app.selected < app.downloads.len() {
                        let dl = &mut app.downloads[app.selected];
                        if !value.is_empty() {
                            // Preserve the original extension if the new name has none
                            let final_name = if let Some(ext) = Path::new(&dl.filename).extension() {
                                if Path::new(&value).extension().is_some() {
                                    value.clone() // user provided an extension
                                } else {
                                    format!("{}.{}", value, ext.to_string_lossy()) // append original extension
                                }
                            } else {
                                value.clone() // no original extension to preserve
                            };
                            dl.filename = final_name.clone();
                            dl.save_path = app.download_dir.join(&final_name);
                            app.set_status(format!("Renamed to {}", final_name));
                        }
                    }
                    app.input_mode = InputMode::Normal;
                    app.input_buf.clear();
                }
                InputMode::SettingSpeedLimit => {
                    // Parse speed limit: supports k, m, g suffixes (KB/s, MB/s, GB/s)
                    let value = value.to_lowercase();
                    let limit_bps = if value.is_empty() || value == "0" || value == "unlimited" {
                        0
                    } else {
                        parse_speed_input(&value)
                    };
                    if app.selected < app.downloads.len() {
                        let id = app.downloads[app.selected].id;
                        app.set_speed_limit(id, limit_bps);
                    }
                    app.input_mode = InputMode::Normal;
                    app.input_buf.clear();
                }
                InputMode::SettingConcurrency => {
                    let max = value.parse::<usize>().unwrap_or(3);
                    app.set_max_concurrent(max.clamp(1, 10));
                    app.input_mode = InputMode::Normal;
                    app.input_buf.clear();
                }
                InputMode::SettingField(field) => {
                    apply_setting(app, field, &value);
                    app.save_settings();
                    app.input_mode = InputMode::Settings;
                    app.input_buf.clear();
                }
                _ => {}
            }
        }
        KeyCode::Backspace => {
            app.input_buf.pop();
        }
        KeyCode::Char(c) => {
            app.input_buf.push(c);
        }
        _ => {}
    }
}

/// Apply a setting value for the given field index.
fn apply_setting(app: &mut App, field: usize, value: &str) {
    match field {
        0 => {
            // Max concurrent
            if let Ok(n) = value.parse::<usize>() {
                app.settings.max_concurrent = n.clamp(1, 10);
                app.max_concurrent = app.settings.max_concurrent;
                app.set_status(format!("🔀 Max concurrent: {}", app.settings.max_concurrent));
                app.process_queue();
            }
        }
        1 => {
            // Default speed limit
            if value.is_empty() || value == "0" || value.to_lowercase() == "unlimited" {
                app.settings.default_speed_limit = None;
            } else {
                let bps = parse_speed_input(value);
                app.settings.default_speed_limit = if bps > 0 { Some(bps) } else { None };
            }
            let msg = match app.settings.default_speed_limit {
                Some(bps) => format!("⚡ Default limit: {}", app::format_speed_simple(bps)),
                None => "⚡ Default limit: Unlimited".to_string(),
            };
            app.set_status(msg);
        }
        2 => {
            // Download directory — create recursively if it doesn't exist
            if !value.is_empty() {
                let path = PathBuf::from(value);
                if path.is_dir() {
                    // Directory already exists, just set it
                    app.settings.download_dir = path.clone();
                    app.download_dir = path;
                    app.set_status(format!("📁 Download dir: {}", app.settings.download_dir.display()));
                } else {
                    // Try to create the directory recursively
                    match std::fs::create_dir_all(&path) {
                        Ok(()) => {
                            app.settings.download_dir = path.clone();
                            app.download_dir = path;
                            app.set_status(format!("📁 Download dir created: {}", app.settings.download_dir.display()));
                        }
                        Err(e) => {
                            app.set_status(format!("⚠ Cannot create directory: {} ({})", value, e));
                        }
                    }
                }
            }
        }
        3 => {
            // Auto-verify
            let v = value.to_lowercase();
            let on = v == "on" || v == "1" || v == "true" || v == "yes";
            let off = v == "off" || v == "0" || v == "false" || v == "no";
            if on { app.settings.auto_verify = true; }
            if off { app.settings.auto_verify = false; }
            app.set_status(format!("🔍 Auto-verify: {}", if app.settings.auto_verify { "On" } else { "Off" }));
        }
        _ => {}
    }
}

/// Parse a speed input string like "100k", "5m", "1g" into bytes/sec.
fn parse_speed_input(s: &str) -> u64 {
    let s = s.trim();
    if s.is_empty() {
        return 0;
    }

    let (num_str, multiplier) = if let Some(stripped) = s.strip_suffix('g') {
        (stripped, 1024 * 1024 * 1024)
    } else if let Some(stripped) = s.strip_suffix('m') {
        (stripped, 1024 * 1024)
    } else if let Some(stripped) = s.strip_suffix('k') {
        (stripped, 1024)
    } else {
        (s, 1)
    };

    num_str
        .trim()
        .parse::<f64>()
        .map(|n| (n * multiplier as f64) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_speed_input ──────────────────────────────────────────────

    #[test]
    fn parse_empty() {
        assert_eq!(parse_speed_input(""), 0);
    }

    #[test]
    fn parse_zero() {
        assert_eq!(parse_speed_input("0"), 0);
    }

    #[test]
    fn parse_plain_number() {
        assert_eq!(parse_speed_input("100"), 100);
        assert_eq!(parse_speed_input("1024"), 1024);
    }

    #[test]
    fn parse_kilobytes() {
        assert_eq!(parse_speed_input("1k"), 1024);
        assert_eq!(parse_speed_input("10k"), 10240);
        assert_eq!(parse_speed_input("500k"), 512000);
    }

    #[test]
    fn parse_megabytes() {
        assert_eq!(parse_speed_input("1m"), 1024 * 1024);
        assert_eq!(parse_speed_input("10m"), 10 * 1024 * 1024);
        assert_eq!(parse_speed_input("100m"), 100 * 1024 * 1024);
    }

    #[test]
    fn parse_gigabytes() {
        assert_eq!(parse_speed_input("1g"), 1024 * 1024 * 1024);
        assert_eq!(parse_speed_input("2g"), 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn parse_invalid() {
        assert_eq!(parse_speed_input("abc"), 0);
        assert_eq!(parse_speed_input("1.2.3"), 0);
        assert_eq!(parse_speed_input("5k"), 5120);
    }

    #[test]
    fn parse_with_spaces() {
        assert_eq!(parse_speed_input("  100  "), 100);
        assert_eq!(parse_speed_input("  10m  "), 10 * 1024 * 1024);
    }

    #[test]
    fn parse_float_values() {
        assert_eq!(parse_speed_input("1.5k"), 1536);
        assert_eq!(parse_speed_input("2.5m"), 2621440);
    }

    #[test]
    fn parse_negative_number() {
        assert_eq!(parse_speed_input("-5m"), 0);
    }

    #[test]
    fn parse_negative_plain() {
        assert_eq!(parse_speed_input("-100"), 0);
    }

    #[test]
    fn parse_whitespace_only() {
        assert_eq!(parse_speed_input("   "), 0);
    }

    #[test]
    fn parse_uppercase_suffix() {
        // Case-sensitive: 'K' != 'k', so it falls through to plain parse and fails
        assert_eq!(parse_speed_input("10K"), 0);
        assert_eq!(parse_speed_input("10M"), 0);
        assert_eq!(parse_speed_input("10G"), 0);
    }

    #[test]
    fn parse_mixed_invalid() {
        // "10x" -> no suffix match -> tries "10x" as float -> fails -> 0
        assert_eq!(parse_speed_input("10x"), 0);
        assert_eq!(parse_speed_input("5mb"), 0);
    }

    #[test]
    fn parse_large_values() {
        assert_eq!(parse_speed_input("1000g"), 1000 * 1024 * 1024 * 1024);
    }

    #[test]
    fn parse_tiny_float() {
        assert_eq!(parse_speed_input("0.1k"), 102);
    }

    #[test]
    fn parse_only_suffix() {
        // "k" alone -> parses as NaN -> 0
        assert_eq!(parse_speed_input("k"), 0);
        assert_eq!(parse_speed_input("m"), 0);
        assert_eq!(parse_speed_input("g"), 0);
    }

    #[test]
    fn parse_dot_without_number() {
        assert_eq!(parse_speed_input("."), 0);
        assert_eq!(parse_speed_input(".k"), 0);
    }
}
