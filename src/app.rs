use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

const NUM_CHUNKS: usize = 4;
const CHUNK_SIZE: u64 = 1024 * 1024 * 2; // 2 MB per chunk
/// Number of editable settings fields.
pub const NUM_SETTINGS: usize = 4;

/// Default settings applied to all new downloads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadSettings {
    pub default_speed_limit: Option<u64>, // bytes/sec, None = unlimited
    pub max_concurrent: usize,
    pub download_dir: PathBuf,
    pub auto_verify: bool,
}

impl Default for DownloadSettings {
    fn default() -> Self {
        let download_dir = dirs::download_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Downloads");
        Self {
            default_speed_limit: None,
            max_concurrent: 3,
            download_dir,
            auto_verify: true,
        }
    }
}

/// Status of a download
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DownloadStatus {
    Pending,
    Downloading,
    Paused,
    Completed,
    Failed(String),
}

/// Verification status for a download
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum VerifyStatus {
    #[default]
    NotChecked,
    Pass,
    Fail,
    NoHash,
}

/// A single download item
#[derive(Debug, Clone)]
pub struct DownloadItem {
    pub id: usize,
    pub url: String,
    pub filename: String,
    pub save_path: PathBuf,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub status: DownloadStatus,
    pub speed: f64,          // bytes per second
    pub start_time: Option<Instant>,
    pub elapsed: Duration,
    pub error: Option<String>,
    pub num_chunks: usize,
    pub chunk_downloaded: Vec<u64>,
    pub supports_range: bool,
    pub expected_sha256: Option<String>,
    pub verify_status: VerifyStatus,
    pub speed_limit: Option<u64>, // bytes/sec, None = unlimited
    pub cancel_token: Arc<AtomicBool>,
    pub created_at: Instant,
}

/// Return a single safe filename component.  Download URLs and HTTP headers are
/// untrusted, so neither may choose a path outside the configured directory.
pub(crate) fn safe_filename(name: &str, fallback: String) -> String {
    let normalized = name.replace('\\', "/");
    let candidate = normalized.rsplit('/').next().unwrap_or_default().trim();
    if candidate.is_empty() || candidate == "." || candidate == ".." || candidate.contains('\0') {
        fallback
    } else {
        candidate.to_string()
    }
}

impl DownloadItem {
    pub fn new(id: usize, url: String, save_path: PathBuf) -> Self {
        let filename = save_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("download_{}", id));

        Self {
            id,
            url,
            filename,
            save_path,
            total_bytes: None,
            downloaded_bytes: 0,
            status: DownloadStatus::Pending,
            speed: 0.0,
            start_time: None,
            elapsed: Duration::ZERO,
            error: None,
            num_chunks: 1,
            chunk_downloaded: Vec::new(),
            supports_range: false,
            expected_sha256: None,
            verify_status: VerifyStatus::NotChecked,
            speed_limit: None,
            cancel_token: Arc::new(AtomicBool::new(false)),
            created_at: Instant::now(),
        }
    }

    pub fn progress(&self) -> f64 {
        match self.total_bytes {
            Some(total) if total > 0 => {
                (self.downloaded_bytes as f64 / total as f64 * 100.0).min(100.0)
            }
            _ => 0.0,
        }
    }

    /// Format downloaded bytes for display (e.g. "12.5 MB")
    pub fn format_downloaded(&self) -> String {
        format_size(self.downloaded_bytes)
    }

    /// Whether we know the total file size
    pub fn has_total(&self) -> bool {
        self.total_bytes.is_some_and(|t| t > 0)
    }

    pub fn chunk_progress(&self, chunk_idx: usize) -> f64 {
        if chunk_idx >= self.chunk_downloaded.len() {
            return 0.0;
        }
        if self.num_chunks <= 1 || self.total_bytes.is_none() {
            return 0.0;
        }
        let total = self.total_bytes.unwrap();
        let chunk_size = (total / self.num_chunks as u64).max(1);
        let expected = if chunk_idx < self.num_chunks - 1 {
            chunk_size
        } else {
            total - chunk_size * (self.num_chunks as u64 - 1)
        };
        if expected == 0 {
            return 100.0;
        }
        (self.chunk_downloaded[chunk_idx] as f64 / expected as f64 * 100.0).min(100.0)
    }
}

/// Message from download workers to the UI
#[derive(Debug)]
pub enum DownloadEvent {
    Progress {
        id: usize,
        downloaded: u64,
        total: Option<u64>,
        speed: f64,
    },
    Completed {
        id: usize,
    },
    Failed {
        id: usize,
        error: String,
    },
    Metadata {
        id: usize,
        filename: String,
        total: Option<u64>,
    },
    Chunks {
        id: usize,
        num_chunks: usize,
        supports_range: bool,
    },
    /// Per-chunk progress update (sent before task exits on cancel/completion)
    ChunkProgress {
        id: usize,
        chunk_idx: usize,
        bytes_done: u64,
    },
}

/// Input mode for the app
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    AddingUrl,
    AddingName,
    AddingSha256,
    Previewing,
    SettingSpeedLimit,
    SettingConcurrency,
    ConfirmDelete,
    Help,
    RenameFile,
    DirectoryBrowser,
    StatsDashboard,
    Settings,
    SettingField(usize),
}

/// Preview info from a HEAD request before starting a download
#[derive(Debug, Clone)]
pub struct DownloadPreview {
    pub url: String,
    pub filename: String,
    pub total_bytes: Option<u64>,
    pub content_type: Option<String>,
    pub supports_range: bool,
}

/// Parameters for spawning a download task.
struct DownloadTaskParams {
    id: usize,
    url_str: String,
    path: PathBuf,
    tx: mpsc::UnboundedSender<DownloadEvent>,
    speed_limit: Option<u64>,
    cancel_token: Arc<AtomicBool>,
    resume_bytes: u64,
    resume_chunk_progress: Vec<u64>,
    saved_total: Option<u64>,
    saved_supports_range: bool,
}

/// Parameters for a single chunk download.
struct ChunkParams {
    client: reqwest::Client,
    url: String,
    path: PathBuf,
    chunk_idx: usize,
    start: u64,
    end: u64,
    downloaded: Arc<AtomicU64>,
    speed_limit: Option<u64>,
    cancel_token: Arc<AtomicBool>,
}

/// A completed download record for history/stats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadHistoryEntry {
    pub filename: String,
    pub url: String,
    pub size_bytes: u64,
    pub completed_at: String, // ISO 8601 timestamp
    pub duration_secs: u64,
}

/// A directory entry in the file browser
#[derive(Debug, Clone)]
pub struct BrowserEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// Main application state
pub struct App {
    pub downloads: Vec<DownloadItem>,
    pub selected: usize,
    pub input_mode: InputMode,
    pub input_buf: String,
    pub pending_url: Option<String>,
    pub pending_filename: Option<String>,
    pub event_tx: mpsc::UnboundedSender<DownloadEvent>,
    pub event_rx: mpsc::UnboundedReceiver<DownloadEvent>,
    pub running: bool,
    pub status_message: Option<String>,
    pub total_completed: usize,
    pub download_dir: PathBuf,
    next_id: usize,
    pub browser_path: PathBuf,
    pub browser_entries: Vec<BrowserEntry>,
    pub browser_selected: usize,
    // Stats
    pub total_bytes_downloaded: u64,
    pub session_start: Instant,
    pub history: Vec<DownloadHistoryEntry>,
    pub history_scroll: usize,
    pub help_scroll: usize,
    // Queue
    pub max_concurrent: usize,
    // Settings
    pub settings: DownloadSettings,
    pub settings_selected: usize,
    // Animation
    pub last_selection_change: Instant,
    // Auto-save
    pub last_auto_save: Instant,
    // Preview
    pub preview: Option<DownloadPreview>,
    pub pending_sha256: Option<String>,
    pub pending_filename_stored: Option<String>,
    pub pending_preview_rx: Option<tokio::sync::mpsc::UnboundedReceiver<DownloadPreview>>,
    pub preview_ready: bool,
}

/// Extract filename from a Content-Disposition header.
fn extract_filename(headers: &reqwest::header::HeaderMap) -> String {
    headers
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .and_then(|cd| {
            cd.split(';')
                .map(|part| part.trim())
                .find_map(|part| {
                    part.strip_prefix("filename=")
                        .map(|name| {
                            name.trim()
                                .trim_start_matches("filename=")
                                .trim_matches('"')
                                .trim_matches('\'')
                                .to_string()
                        })
                })
        })
        .unwrap_or_default()
}

/// Standalone async HEAD request to fetch file info for the preview modal.
/// Separated from App because we can't hold &self across await points.
pub(crate) async fn fetch_head_info(url: &str) -> DownloadPreview {
    let mut preview = DownloadPreview {
        url: url.to_string(),
        filename: String::new(),
        total_bytes: None,
        content_type: None,
        supports_range: false,
    };

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(_) => return preview,
    };

    // Try HEAD request first
    match client.head(url).send().await {
        Ok(r) if r.status().is_success() => {
            preview.total_bytes = r.headers().get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|&v| v > 0)
                .or_else(|| r.content_length());
            preview.content_type = r.headers().get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.split(';').next().unwrap_or(s).trim().to_string());
            preview.supports_range = r.headers().get("accept-ranges")
                .and_then(|v| v.to_str().ok())
                .map(|v| v == "bytes")
                .unwrap_or(false);
            let name = extract_filename(r.headers());
            if !name.is_empty() {
                preview.filename = name;
            }
        }
        _ => {
            // HEAD failed — try GET Range probe
            if let Ok(r) = client.get(url).header("Range", "bytes=0-0").send().await {
                if r.status().as_u16() == 206 {
                    preview.supports_range = true;
                    preview.total_bytes = r.headers().get("content-range")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|cr| cr.split('/').next_back())
                        .filter(|s| *s != "*")
                        .and_then(|s| s.parse::<u64>().ok());
                    preview.content_type = r.headers().get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string());
                    let name = extract_filename(r.headers());
                    if !name.is_empty() {
                        preview.filename = name;
                    }
                }
            }
        }
    }

    preview
}

impl App {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let download_dir = dirs::download_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Downloads");

        let _ = std::fs::create_dir_all(&download_dir);

        let mut app = Self {
            downloads: Vec::new(),
            selected: 0,
            input_mode: InputMode::Normal,
            input_buf: String::new(),
            pending_url: None,
            pending_filename: None,
            event_tx: tx,
            event_rx: rx,
            running: true,
            status_message: None,
            total_completed: 0,
            download_dir: download_dir.clone(),
            next_id: 1,
            browser_path: download_dir,
            browser_entries: Vec::new(),
            browser_selected: 0,
            total_bytes_downloaded: 0,
            session_start: Instant::now(),
            history: Vec::new(),
            history_scroll: 0,
            help_scroll: 0,
            max_concurrent: 3,
            settings: DownloadSettings::default(),
            settings_selected: 0,
            last_selection_change: Instant::now(),
            last_auto_save: Instant::now(),
            preview: None,
            pending_sha256: None,
            pending_filename_stored: None,
            pending_preview_rx: None,
            preview_ready: false,
        };

        // Load persisted state from previous session
        app.load_state();
        app.load_history();
        app.load_settings();
        // Apply settings
        app.max_concurrent = app.settings.max_concurrent;
        app.download_dir = app.settings.download_dir.clone();
        app.auto_resume_incomplete();

        app
    }

    pub fn add_download(&mut self, url: String, filename: Option<String>, sha256: Option<String>) {
        let fallback = format!("download_{}", self.next_id);
        let requested_name = filename.unwrap_or_else(|| self.filename_from_url(&url));
        let filename = safe_filename(&requested_name, fallback);
        let save_path = self.download_dir.join(&filename);

        let mut item = DownloadItem::new(self.next_id, url, save_path);
        item.expected_sha256 = sha256.filter(|s| !s.is_empty());
        if item.expected_sha256.is_some() {
            item.verify_status = VerifyStatus::NotChecked;
        } else {
            item.verify_status = VerifyStatus::NoHash;
        }
        // Apply default speed limit from settings
        item.speed_limit = self.settings.default_speed_limit;
        let id = item.id;
        self.next_id += 1;
        self.downloads.push(item);
        self.set_status(format!("Download #{} queued", id));
    }

    /// Compute SHA-256 of a file and compare against expected hash.
    pub fn verify_download(&mut self, id: usize) {
        // Extract info first to avoid double mutable borrow
        let (status, expected, path, filename) = {
            if let Some(item) = self.downloads.iter().find(|d| d.id == id) {
                (
                    item.status.clone(),
                    item.expected_sha256.clone(),
                    item.save_path.clone(),
                    item.filename.clone(),
                )
            } else {
                return;
            }
        };

        if status != DownloadStatus::Completed {
            if let Some(item) = self.downloads.iter_mut().find(|d| d.id == id) {
                item.verify_status = VerifyStatus::Fail;
                item.error = Some("Download not completed".into());
            }
            return;
        }

        let expected = match expected {
            Some(h) => h,
            None => {
                if let Some(item) = self.downloads.iter_mut().find(|d| d.id == id) {
                    item.verify_status = VerifyStatus::NoHash;
                }
                return;
            }
        };

        match std::fs::read(&path) {
            Ok(data) => {
                use sha2::{Sha256, Digest};
                let mut hasher = Sha256::new();
                hasher.update(&data);
                let computed = hex::encode(hasher.finalize());
                if let Some(item) = self.downloads.iter_mut().find(|d| d.id == id) {
                    if computed.to_lowercase() == expected.to_lowercase() {
                        item.verify_status = VerifyStatus::Pass;
                    } else {
                        item.verify_status = VerifyStatus::Fail;
                    }
                }
                if computed.to_lowercase() == expected.to_lowercase() {
                    self.set_status(format!("✓ {} checksum OK", filename));
                } else {
                    self.set_status(format!("✗ {} checksum MISMATCH", filename));
                }
            }
            Err(e) => {
                if let Some(item) = self.downloads.iter_mut().find(|d| d.id == id) {
                    item.verify_status = VerifyStatus::Fail;
                    item.error = Some(format!("Read error: {}", e));
                }
            }
        }
    }

    /// Set speed limit for a download (bytes/sec). Pass 0 to clear.
    pub fn set_speed_limit(&mut self, id: usize, limit_bps: u64) {
        if let Some(item) = self.downloads.iter_mut().find(|d| d.id == id) {
            item.speed_limit = if limit_bps > 0 { Some(limit_bps) } else { None };
            let msg = if limit_bps > 0 {
                format!("⚡ {} limit: {}", item.filename, format_speed_simple(limit_bps))
            } else {
                format!("⚡ {} limit removed", item.filename)
            };
            self.set_status(msg);
        }
    }

    pub(crate) fn filename_from_url(&self, url_str: &str) -> String {
        if let Ok(parsed) = url::Url::parse(url_str) {
            let path = parsed.path();
            let name = path
                .rsplit('/')
                .next()
                .unwrap_or("download")
                .to_string();
            if name.is_empty() || name == "/" {
                format!("download_{}", self.next_id)
            } else {
                // Simple percent-decode without extra dependency.  Keep the
                // result to one path component before using it as a filename.
                safe_filename(
                    &name.replace("%20", " ").replace("%2F", "/"),
                    format!("download_{}", self.next_id),
                )
            }
        } else {
            format!("download_{}", self.next_id)
        }
    }

    pub fn start_download(&mut self, id: usize) {
        // Drain pending events first — ensures chunk_downloaded has latest
        // ChunkProgress values from the previous task before we clone them.
        self.poll_events();

        // Check concurrent limit
        let active_count = self.downloads.iter()
            .filter(|d| d.status == DownloadStatus::Downloading)
            .count();

        // Check if should queue
        if active_count >= self.max_concurrent {
            if let Some(item) = self.downloads.iter_mut().find(|d| d.id == id) {
                if item.status == DownloadStatus::Pending || item.status == DownloadStatus::Paused {
                    item.status = DownloadStatus::Pending;
                    let filename = item.filename.clone();
                    let queued = self.downloads.iter().filter(|d| d.status == DownloadStatus::Pending).count();
                    self.set_status(format!("📋 {} queued (position {})", filename, queued));
                    return;
                }
            }
            return;
        }

        if let Some(item) = self.downloads.iter_mut().find(|d| d.id == id) {
            if item.status == DownloadStatus::Pending || item.status == DownloadStatus::Paused {
                // For Pending (brand-new) downloads, force progress to 0%
                // even if stale events set downloaded_bytes earlier.
                let is_pending = item.status == DownloadStatus::Pending;
                item.status = DownloadStatus::Downloading;
                item.start_time = Some(Instant::now());
                if is_pending {
                    item.downloaded_bytes = 0;
                    item.total_bytes = None;
                    item.chunk_downloaded.clear();
                    item.supports_range = false;
                }

                let tx = self.event_tx.clone();
                let url = item.url.clone();
                let path = item.save_path.clone();
                let id = item.id;
                let speed_limit = item.speed_limit;
                let cancel_token = item.cancel_token.clone();
                let resume_bytes = item.downloaded_bytes;
                let resume_chunk_progress = item.chunk_downloaded.clone();
                let saved_total = item.total_bytes;
                let saved_supports_range = item.supports_range;

                let params = DownloadTaskParams {
                    id, url_str: url, path, tx, speed_limit,
                    cancel_token, resume_bytes, resume_chunk_progress,
                    saved_total, saved_supports_range,
                };
                tokio::spawn(async move {
                    Self::download_task(params).await;
                });
            }
        }
    }

    /// Start the next queued download if under the concurrent limit.
    pub fn process_queue(&mut self) {
        let active_count = self.downloads.iter()
            .filter(|d| d.status == DownloadStatus::Downloading)
            .count();

        if active_count >= self.max_concurrent {
            return;
        }

        let slots = self.max_concurrent - active_count;
        let pending_ids: Vec<usize> = self.downloads.iter()
            .filter(|d| d.status == DownloadStatus::Pending)
            .map(|d| d.id)
            .take(slots)
            .collect();

        for id in pending_ids {
            self.start_download(id);
        }
    }

    /// Set max concurrent downloads.
    pub fn set_max_concurrent(&mut self, max: usize) {
        self.max_concurrent = max.clamp(1, 10);
        self.set_status(format!("🔀 Max concurrent: {}", self.max_concurrent));
        self.process_queue();
    }

    /// Open the containing folder of a download in the system file manager.
    pub fn open_folder(&mut self, id: usize) {
        if let Some(item) = self.downloads.iter().find(|d| d.id == id) {
            let dir = if item.save_path.is_dir() {
                item.save_path.clone()
            } else {
                item.save_path.parent().unwrap_or(&item.save_path).to_path_buf()
            };

            #[cfg(target_os = "macos")]
            let _ = std::process::Command::new("open")
                .arg(&dir)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();

            #[cfg(target_os = "linux")]
            let _ = std::process::Command::new("xdg-open")
                .arg(&dir)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();

            #[cfg(target_os = "windows")]
            let _ = std::process::Command::new("explorer")
                .arg(&dir)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();

            self.set_status(format!("📂 Opened {}", dir.display()));
        }
    }

    pub fn pause_download(&mut self, id: usize) {
        if let Some(item) = self.downloads.iter_mut().find(|d| d.id == id) {
            if item.status == DownloadStatus::Downloading {
                item.cancel_token.store(true, Ordering::Relaxed);
                item.status = DownloadStatus::Paused;
                item.speed = 0.0;
                item.elapsed += item
                    .start_time
                    .map(|t| t.elapsed())
                    .unwrap_or_default();
                // Note: downloaded_bytes is updated from disk by the progress reporter
                // and by save_state, so don't try to read it here — the chunk tasks
                // may not have flushed yet.
            }
        }
    }

    pub fn resume_download(&mut self, id: usize) {
        if let Some(item) = self.downloads.iter_mut().find(|d| d.id == id) {
            if item.status == DownloadStatus::Paused {
                // Create a fresh cancel token (reset from cancelled state)
                item.cancel_token = Arc::new(AtomicBool::new(false));
                // Don't set status here — start_download will do it
                // start_download checks for Paused status, so leave it as Paused
                self.start_download(id);
            }
        }
    }

    pub fn remove_download(&mut self, id: usize) {
        self.downloads.retain(|d| d.id != id);
        if self.selected >= self.downloads.len() && self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Clear all completed and failed downloads from the list.
    pub fn clear_completed(&mut self) {
        let before = self.downloads.len();
        self.downloads.retain(|d| {
            matches!(d.status, DownloadStatus::Downloading | DownloadStatus::Pending | DownloadStatus::Paused)
        });
        let cleared = before - self.downloads.len();
        if self.selected >= self.downloads.len() && self.selected > 0 {
            self.selected = self.downloads.len().saturating_sub(1);
        }
        if cleared > 0 {
            self.set_status(format!("🗑 Cleared {} download(s)", cleared));
        } else {
            self.set_status("Nothing to clear".to_string());
        }
    }

    pub fn retry_download(&mut self, id: usize) {
        // Drain stale events from the previous task FIRST, so their
        // Progress(downloaded=…) values are consumed before we reset state.
        self.poll_events();
        if let Some(item) = self.downloads.iter_mut().find(|d| d.id == id) {
            item.status = DownloadStatus::Pending;
            item.downloaded_bytes = 0;
            item.total_bytes = None;
            item.speed = 0.0;
            item.error = None;
            item.elapsed = Duration::ZERO;
            item.start_time = None;
            item.chunk_downloaded.clear();
            item.supports_range = false;
            // Delete the partial file so progress starts at 0%
            let _ = std::fs::remove_file(&item.save_path);
            self.start_download(id);
        }
    }

    pub fn move_selection_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.last_selection_change = Instant::now();
        }
    }

    pub fn move_selection_down(&mut self) {
        if self.selected + 1 < self.downloads.len() {
            self.selected += 1;
            self.last_selection_change = Instant::now();
        }
    }

    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some(msg);
    }

    // ── Directory Browser ─────────────────────────────────────────────────

    pub fn open_browser(&mut self) {
        self.input_mode = InputMode::DirectoryBrowser;
        self.browser_path = self.download_dir.clone();
        self.browser_selected = 0;
        self.refresh_browser_entries();
    }

    pub fn refresh_browser_entries(&mut self) {
        self.browser_entries.clear();
        self.browser_selected = 0;

        // Add parent directory entry if not at root
        if let Some(parent) = self.browser_path.parent() {
            self.browser_entries.push(BrowserEntry {
                name: "..".to_string(),
                path: parent.to_path_buf(),
                is_dir: true,
            });
        }

        // Read directory entries
        if let Ok(entries) = std::fs::read_dir(&self.browser_path) {
            let mut dirs: Vec<BrowserEntry> = Vec::new();
            let mut files: Vec<BrowserEntry> = Vec::new();

            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                // Skip hidden files
                if name.starts_with('.') {
                    continue;
                }

                let is_dir = path.is_dir();
                let entry = BrowserEntry {
                    name,
                    path,
                    is_dir,
                };

                if is_dir {
                    dirs.push(entry);
                } else {
                    files.push(entry);
                }
            }

            // Sort alphabetically
            dirs.sort_by_key(|a| a.name.to_lowercase());
            files.sort_by_key(|a| a.name.to_lowercase());

            self.browser_entries.extend(dirs);
            self.browser_entries.extend(files);
        }
    }

    pub fn browser_select_up(&mut self) {
        if self.browser_selected > 0 {
            self.browser_selected -= 1;
        }
    }

    pub fn browser_select_down(&mut self) {
        if self.browser_selected + 1 < self.browser_entries.len() {
            self.browser_selected += 1;
        }
    }

    pub fn browser_enter(&mut self) {
        if self.browser_selected >= self.browser_entries.len() {
            return;
        }

        let entry = &self.browser_entries[self.browser_selected];
        if entry.is_dir {
            self.browser_path = entry.path.clone();
            self.refresh_browser_entries();
        }
    }

    pub fn browser_go_up(&mut self) {
        if let Some(parent) = self.browser_path.parent() {
            self.browser_path = parent.to_path_buf();
            self.refresh_browser_entries();
        }
    }

    pub fn browser_select_current(&mut self) {
        self.download_dir = self.browser_path.clone();
        self.settings.download_dir = self.download_dir.clone();
        self.input_mode = InputMode::Normal;
        self.set_status(format!("📁 Download dir: {}", self.download_dir.display()));
    }

    /// Rename a queued, paused, completed, or failed download and its file.
    /// Active downloads keep an open path in worker tasks, so they must be
    /// paused before renaming.
    pub fn rename_download(&mut self, id: usize, requested_name: &str) -> Result<String, String> {
        let filename = safe_filename(requested_name, String::new());
        if filename.is_empty() || filename != requested_name {
            return Err("Filename must not contain path separators".into());
        }

        let item = self
            .downloads
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| "Download not found".to_string())?;
        if item.status == DownloadStatus::Downloading {
            return Err("Pause the download before renaming it".into());
        }

        let old_path = item.save_path.clone();
        let parent = old_path.parent().unwrap_or_else(|| Path::new("."));
        let new_path = parent.join(&filename);
        if new_path != old_path && new_path.exists() {
            return Err("A file with that name already exists".into());
        }
        if new_path != old_path && old_path.exists() {
            std::fs::rename(&old_path, &new_path)
                .map_err(|error| format!("Could not rename file: {error}"))?;
        }
        item.filename = filename.clone();
        item.save_path = new_path;
        Ok(filename)
    }

    // ── Clipboard Paste ───────────────────────────────────────────────────

    /// Read clipboard and start downloading if it contains a URL.
    /// Returns true if a download was started.
    pub fn paste_from_clipboard(&mut self) -> bool {
        let text = match arboard::Clipboard::new() {
            Ok(mut cb) => cb.get_text().unwrap_or_default().to_string(),
            Err(_) => {
                self.set_status("⚠ Cannot access clipboard".into());
                return false;
            }
        };

        let text = text.trim().to_string();

        if text.is_empty() {
            self.set_status("📋 Clipboard is empty".into());
            return false;
        }

        // Check if it looks like a URL (one or more lines)
        let urls: Vec<String> = text
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| l.starts_with("http://") || l.starts_with("https://"))
            .collect();

        if urls.is_empty() {
            self.set_status(format!("📋 Not a URL: {}", truncate_str(&text, 40)));
            return false;
        }

        for url in urls {
            self.add_download(url, None, None);
            let id = self.downloads.last().map(|d| d.id).unwrap_or(0);
            self.start_download(id);
        }

        true
    }

    // ── Notification ──────────────────────────────────────────────────────

    /// Play a notification sound when a download completes.
    /// Uses terminal BEL (works everywhere) + macOS system sound for extra polish.
    fn notify_completion(&self) {
        // Terminal BEL character — produces the system bell on all platforms
        let _ = std::io::Write::write_all(&mut std::io::stdout(), b"\x07");
        let _ = std::io::Write::flush(&mut std::io::stdout());

        // On macOS, also play a system sound for a nicer notification
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("afplay")
                .arg("/System/Library/Sounds/Glass.aiff")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
    }

    fn update_downloads_from_event(&mut self, event: DownloadEvent) {
        match event {
            DownloadEvent::Progress {
                id,
                downloaded,
                total,
                speed,
            } => {
                if let Some(item) = self.downloads.iter_mut().find(|d| d.id == id) {
                    item.downloaded_bytes = downloaded;
                    // Only upgrade total_bytes — never downgrade it.
                    // If we already have a valid total from a previous probe or session,
                    // don't overwrite it with None or Some(0) from the progress reporter.
                    let new_total_is_valid = total.is_some_and(|t| t > 0);
                    let old_total_is_valid = item.total_bytes.is_some_and(|t| t > 0);
                    if new_total_is_valid || !old_total_is_valid {
                        item.total_bytes = total;
                    }
                    item.speed = speed;
                }
            }
            DownloadEvent::Completed { id } => {
                let (entry, should_auto_verify) = if let Some(item) = self.downloads.iter_mut().find(|d| d.id == id) {
                    item.status = DownloadStatus::Completed;
                    item.speed = 0.0;
                    item.elapsed += item.start_time.take().map(|started| started.elapsed()).unwrap_or_default();
                    self.total_completed += 1;
                    self.total_bytes_downloaded += item.downloaded_bytes;
                    let duration = item.elapsed.as_secs();
                    (
                        Some(DownloadHistoryEntry {
                            filename: item.filename.clone(),
                            url: item.url.clone(),
                            size_bytes: item.downloaded_bytes,
                            completed_at: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
                            duration_secs: duration,
                        }),
                        self.settings.auto_verify && item.expected_sha256.is_some(),
                    )
                } else {
                    (None, false)
                };
                if let Some(entry) = entry {
                    self.history.insert(0, entry);
                    // Keep last 100 entries
                    if self.history.len() > 100 {
                        self.history.truncate(100);
                    }
                    self.save_history();
                    self.set_status(format!("✓ {} completed", self.history[0].filename));
                }
                if should_auto_verify {
                    self.verify_download(id);
                }
                self.notify_completion();
                self.process_queue();
            }
            DownloadEvent::Failed { id, error } => {
                let msg = if let Some(item) = self.downloads.iter_mut().find(|d| d.id == id) {
                    item.status = DownloadStatus::Failed(error.clone());
                    item.speed = 0.0;

                    Some(format!("✗ {} failed: {}", item.filename.clone(), error))
                } else {
                    None
                };
                if let Some(msg) = msg {
                    self.set_status(msg);
                }
                self.process_queue();
            }
            DownloadEvent::Metadata {
                id,
                filename,
                total,
            } => {
                // The destination is fixed before workers start.  Previewing
                // already applies a server filename when the user accepts it;
                // changing it here would desynchronize the worker's path.
                let _ = filename;
                if let Some(item) = self.downloads.iter_mut().find(|d| d.id == id) {
                    // Only upgrade total_bytes — never downgrade it.
                    // A resumed download already has the correct total from the previous session.
                    let new_total_is_valid = total.is_some_and(|t| t > 0);
                    let old_total_is_valid = item.total_bytes.is_some_and(|t| t > 0);
                    if new_total_is_valid || !old_total_is_valid {
                        item.total_bytes = total;
                    }
                }
            }
            DownloadEvent::Chunks {
                id,
                num_chunks,
                supports_range,
            } => {
                if let Some(item) = self.downloads.iter_mut().find(|d| d.id == id) {
                    item.num_chunks = num_chunks;
                    item.supports_range = supports_range;
                    // Only reset chunk_downloaded if length doesn't match (new download)
                    // Don't overwrite saved per-chunk progress from a previous session!
                    if item.chunk_downloaded.len() != num_chunks {
                        item.chunk_downloaded = vec![0; num_chunks];
                    }
                }
            }
            DownloadEvent::ChunkProgress {
                id,
                chunk_idx,
                bytes_done,
            } => {
                if let Some(item) = self.downloads.iter_mut().find(|d| d.id == id) {
                    if chunk_idx < item.chunk_downloaded.len() {
                        item.chunk_downloaded[chunk_idx] = bytes_done;
                    }
                }
            }
        }
    }

    pub fn poll_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            self.update_downloads_from_event(event);
        }
    }

    async fn download_task(p: DownloadTaskParams) {
        let DownloadTaskParams {
            id, url_str, path, tx, speed_limit,
            cancel_token, resume_bytes, resume_chunk_progress,
            saved_total, saved_supports_range,
        } = p;
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(DownloadEvent::Failed {
                    id,
                    error: e.to_string(),
                });
                return;
            }
        };

        // ── Probe: learn file size + range support ──────────────────────
        // Step 1: HEAD (instant, no body) → Content-Length + Accept-Ranges.
        // Step 2: If HEAD gave no Content-Length, GET Range: bytes=0-0 (1 byte)
        //         → extract total from Content-Range header.
        // If neither gives a size, download proceeds in single-stream mode.
        use futures_util::StreamExt;

        let mut total: Option<u64> = None;
        let mut supports_range = false;

        // Try 1: HEAD request (instant — no body transferred)
        let mut got_size = false;
        match client.head(&url_str).send().await {
            Ok(r) if r.status().is_success() => {
                // Parse content-length manually — reqwest's content_length() can
                // return None or 0 on HTTP/2 HEAD responses even when the header is present.
                total = r.headers().get("content-length")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .filter(|&v| v > 0)
                    .or_else(|| r.content_length());
                supports_range = r.headers().get("accept-ranges")
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v == "bytes")
                    .unwrap_or(false);
                let filename = extract_filename(r.headers());
                if !filename.is_empty() {
                    let _ = tx.send(DownloadEvent::Metadata { id, filename, total });
                } else {
                    let _ = tx.send(DownloadEvent::Metadata { id, filename: String::new(), total });
                }
                got_size = total.is_some();
            }
            _ => {
                // HEAD failed — fall through to Range probe
            }
        }

        // Try 2: GET Range: bytes=0-0 (1 byte) — only if HEAD didn't give us a size
        if !got_size {
            match client
                .get(&url_str)
                .header("Range", "bytes=0-0")
                .send()
                .await
            {
                Ok(r) if r.status().as_u16() == 206 => {
                    // 206 Partial Content — ranges supported!
                    supports_range = true;
                    // Content-Range: bytes 0-0/TOTAL
                    if let Some(cr) = r.headers().get("content-range") {
                        if let Ok(cr_str) = cr.to_str() {
                            if let Some(total_str) = cr_str.split('/').next_back() {
                                if total_str != "*" {
                                    total = total_str.parse::<u64>().ok();
                                }
                            }
                        }
                    }
                    let filename = extract_filename(r.headers());
                    if !filename.is_empty() {
                        let _ = tx.send(DownloadEvent::Metadata { id, filename, total });
                    } else {
                        let _ = tx.send(DownloadEvent::Metadata { id, filename: String::new(), total });
                    }
                    // Consume the 1-byte body
                    let mut stream = r.bytes_stream();
                    while stream.next().await.is_some() {}
                }
                Ok(r) if r.status().is_success() => {
                    // 200 OK — server ignored Range, returned full body
                    // We know ranges aren't supported. Get size from Content-Length.
                    total = r.content_length();
                    supports_range = false;
                    let filename = extract_filename(r.headers());
                    if !filename.is_empty() {
                        let _ = tx.send(DownloadEvent::Metadata { id, filename, total });
                    } else {
                        let _ = tx.send(DownloadEvent::Metadata { id, filename: String::new(), total });
                    }
                    // Must consume the body to free the connection.
                    // The actual download will re-fetch via single-stream GET.
                    let mut stream = r.bytes_stream();
                    while stream.next().await.is_some() {}
                }
                _ => {
                    // Both HEAD and Range failed — try one last HEAD without status check
                    if let Ok(r) = client.head(&url_str).send().await {
                        total = r.content_length();
                        let filename = extract_filename(r.headers());
                        if !filename.is_empty() {
                            let _ = tx.send(DownloadEvent::Metadata { id, filename, total });
                        } else {
                            let _ = tx.send(DownloadEvent::Metadata { id, filename: String::new(), total });
                        }
                    }
                }
            }
        }

        // ── Check for resume ─────────────────────────────────────────────
        // If probe failed to get file size, fall back to the saved total from
        // the previous session. This is critical for resume after pause.
        if total.is_none() && saved_total.is_some() {
            total = saved_total;
        }
        if !supports_range && saved_supports_range {
            supports_range = true;
        }
        // If we used the saved total, tell the UI so total_bytes is set correctly
        if total.is_some() && total != Some(0) {
            let _ = tx.send(DownloadEvent::Metadata {
                id,
                filename: String::new(),
                total,
            });
        }
        let file_size = total.unwrap_or(0);
        // Per-chunk counters are the authoritative resume offsets.  They also
        // recover states written by older versions that mistakenly persisted a
        // sparse file's apparent length as downloaded_bytes.
        let saved_chunk_bytes = resume_chunk_progress.iter().fold(0_u64, |total, bytes| {
            total.saturating_add(*bytes)
        });
        let existing_bytes = if saved_chunk_bytes > 0 {
            saved_chunk_bytes
        } else {
            resume_bytes
        };
        let resuming = existing_bytes > 0 && supports_range && existing_bytes <= file_size;

        if resuming {
            let _ = tx.send(DownloadEvent::Progress {
                id,
                downloaded: existing_bytes,
                total: if file_size > 0 { Some(file_size) } else { None },
                speed: 0.0,
            });
        }

        // ── Decide: chunked or single-stream ─────────────────────────────
        let use_chunked = supports_range && file_size > CHUNK_SIZE * 2 && file_size < u64::MAX / 2;
        let num_chunks = if use_chunked {
            let ideal = (file_size / CHUNK_SIZE).max(1) as usize;
            ideal.min(NUM_CHUNKS)
        } else {
            1
        };

        let _ = tx.send(DownloadEvent::Chunks {
            id,
            num_chunks,
            supports_range,
        });

        // ── Ensure parent directory exists ────────────────────────────────
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // ── Ensure file exists (no pre-allocation) ─────────────────────
        // Don't pre-allocate: file size must reflect actual downloaded bytes
        // so resume detection works correctly on next launch.
        if !resuming {
            let _ = std::fs::File::create(&path);
        }

        // ── Spawn chunk tasks ────────────────────────────────────────────
        let chunk_downloaded: Vec<Arc<AtomicU64>> = (0..num_chunks)
            .map(|_| Arc::new(AtomicU64::new(0)))
            .collect();

        // Pre-fill chunk_downloaded with saved per-chunk progress (accurate resume offsets)
        if resuming && num_chunks > 1 {
            if resume_chunk_progress.len() == num_chunks {
                // We have accurate per-chunk progress from previous session
                for i in 0..num_chunks {
                    chunk_downloaded[i].store(resume_chunk_progress[i], Ordering::Relaxed);
                }
            } else {
                // Fallback: distribute evenly (should not happen with proper state saving)
                let chunk_size = (file_size / num_chunks as u64).max(1);
                let mut remaining = existing_bytes;
                for (i, chunk) in chunk_downloaded.iter().enumerate() {
                    let expected = if i < num_chunks - 1 {
                        chunk_size
                    } else {
                        file_size - chunk_size * (num_chunks as u64 - 1)
                    };
                    let done = remaining.min(expected);
                    chunk.store(done, Ordering::Relaxed);
                    remaining = remaining.saturating_sub(done);
                }
            }
        } else if resuming && num_chunks == 1 {
            chunk_downloaded[0].store(existing_bytes, Ordering::Relaxed);
        }

        let mut handles = Vec::with_capacity(num_chunks);

        for (i, chunk_atomic) in chunk_downloaded.iter().enumerate() {
            let client = client.clone();
            let url = url_str.clone();
            let file_path = path.clone();
            let chunk_dl = chunk_atomic.clone();

            let chunk_total = if num_chunks == 1 {
                file_size
            } else {
                let chunk_size = (file_size / num_chunks as u64).max(1);
                if i < num_chunks - 1 {
                    chunk_size
                } else {
                    file_size - chunk_size * (num_chunks as u64 - 1)
                }
            };

            let already_done = chunk_downloaded[i].load(Ordering::Relaxed);
            let start = if num_chunks == 1 {
                already_done
            } else {
                let base = (file_size / num_chunks as u64) * i as u64;
                base + already_done
            };
            let end = if num_chunks == 1 {
                u64::MAX
            } else {
                let base = (file_size / num_chunks as u64) * i as u64;
                base + chunk_total - 1
            };

            // Skip chunks that are already complete
            // But only if we know the file size (chunk_total > 0)
            if chunk_total > 0 && already_done >= chunk_total {
                continue;
            }

            // Divide speed limit among chunks
            let chunk_speed_limit = speed_limit.map(|lim| {
                (lim / num_chunks as u64).max(1)
            });

            let ct = cancel_token.clone();
            let params = ChunkParams {
                client: client.clone(),
                url: url.clone(),
                path: file_path,
                chunk_idx: i,
                start,
                end,
                downloaded: chunk_dl,
                speed_limit: chunk_speed_limit,
                cancel_token: ct,
            };
            let handle = tokio::spawn(async move {
                Self::download_chunk(params).await
            });
            handles.push(handle);
        }

        // ── Progress reporter ─────────────────────────────────────────────
        // Sum completed bytes per chunk.  File length is not reliable here:
        // seeking to a later parallel chunk makes a sparse file appear much
        // larger than the data actually written.
        // Also periodically sends ChunkProgress events so item.chunk_downloaded
        // stays current during active download (needed for accurate resume).
        let tx_clone = tx.clone();
        let chunk_dl_clone = chunk_downloaded.clone();
        let report_handle = tokio::spawn(async move {
            let mut last_bytes: u64 = 0;
            let mut last_time = Instant::now();
            let mut smoothed_speed: f64 = 0.0;
            const ALPHA: f64 = 0.3;
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let downloaded_now = chunk_dl_clone.iter().fold(0_u64, |total, chunk| {
                    total.saturating_add(chunk.load(Ordering::Relaxed))
                });
                let elapsed = last_time.elapsed().as_secs_f64();
                let instant_speed = if elapsed > 0.0 {
                    (downloaded_now.saturating_sub(last_bytes)) as f64 / elapsed
                } else {
                    0.0
                };
                smoothed_speed = ALPHA * instant_speed + (1.0 - ALPHA) * smoothed_speed;
                if smoothed_speed < 1.0 && instant_speed < 1.0 {
                    smoothed_speed = 0.0;
                }
                last_bytes = downloaded_now;
                last_time = Instant::now();
                let _ = tx_clone.send(DownloadEvent::Progress {
                    id,
                    downloaded: downloaded_now,
                    total: if file_size > 0 { Some(file_size) } else { None },
                    speed: smoothed_speed,
                });
                // Send per-chunk progress so item.chunk_downloaded stays current
                for (i, cd) in chunk_dl_clone.iter().enumerate() {
                    let _ = tx_clone.send(DownloadEvent::ChunkProgress {
                        id,
                        chunk_idx: i,
                        bytes_done: cd.load(Ordering::Relaxed),
                    });
                }
            }
        });

        // ── Wait for all chunks to finish ─────────────────────────────────
        let mut any_failed = false;
        let mut last_error = String::new();
        for handle in handles {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    any_failed = true;
                    last_error = e;
                }
                Err(e) => {
                    any_failed = true;
                    last_error = format!("Chunk task panicked: {}", e);
                }
            }
        }

        // Stop the reporter
        report_handle.abort();

        // If cancelled (paused), send final progress from disk then stop
        if cancel_token.load(Ordering::Relaxed) {
            let final_downloaded = chunk_downloaded.iter().fold(0_u64, |total, chunk| {
                total.saturating_add(chunk.load(Ordering::Relaxed))
            });
            let _ = tx.send(DownloadEvent::Progress {
                id,
                downloaded: final_downloaded,
                total: if file_size > 0 { Some(file_size) } else { None },
                speed: 0.0,
            });
            // Send per-chunk progress so it's saved to state for accurate resume
            for (i, cd) in chunk_downloaded.iter().enumerate() {
                let _ = tx.send(DownloadEvent::ChunkProgress {
                    id,
                    chunk_idx: i,
                    bytes_done: cd.load(Ordering::Relaxed),
                });
            }
            return;
        }

        if any_failed {
            let _ = tx.send(DownloadEvent::Failed {
                id,
                error: last_error,
            });
        } else {
            let final_downloaded = chunk_downloaded.iter().fold(0_u64, |total, chunk| {
                total.saturating_add(chunk.load(Ordering::Relaxed))
            });
            if file_size > 0 && final_downloaded != file_size {
                let _ = tx.send(DownloadEvent::Failed {
                    id,
                    error: format!("Downloaded {final_downloaded} of {file_size} expected bytes"),
                });
                return;
            }
            let _ = tx.send(DownloadEvent::Progress {
                id,
                downloaded: final_downloaded,
                total: if file_size > 0 { Some(file_size) } else { None },
                speed: 0.0,
            });
            let _ = tx.send(DownloadEvent::Completed { id });
        }
    }

    async fn download_chunk(p: ChunkParams) -> Result<(), String> {
        let ChunkParams {
            client, url, path, chunk_idx, start, end,
            downloaded, speed_limit, cancel_token,
        } = p;
        use futures_util::StreamExt;

        let range_header = if end == u64::MAX {
            format!("bytes={}-", start)
        } else {
            format!("bytes={}-{}", start, end)
        };

        let response = client
            .get(url)
            .header("Range", &range_header)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let requires_partial_response = start > 0 || end != u64::MAX;
        if !response.status().is_success()
            || (requires_partial_response && response.status() != reqwest::StatusCode::PARTIAL_CONTENT)
        {
            return Err(format!("HTTP {} for chunk {}", response.status(), chunk_idx));
        }

        let mut stream = response.bytes_stream();
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(false)
            .open(&path)
            .await
            .map_err(|e| format!("Failed to open file for chunk {}: {}", chunk_idx, e))?;

        // Rate limiter state: token bucket
        let mut bucket: u64 = 0;
        let mut last_refill = Instant::now();
        let bucket_capacity = speed_limit.unwrap_or(u64::MAX);

        use tokio::io::{AsyncSeekExt, AsyncWriteExt};
        let mut pos = start;
        let mut received = 0_u64;
        while let Some(chunk) = stream.next().await {
            // Check if download was cancelled — flush first!
            if cancel_token.load(Ordering::Relaxed) {
                let _ = file.flush().await;
                return Ok(());
            }

            let bytes = chunk.map_err(|e| e.to_string())?;
            let chunk_len = bytes.len() as u64;

            // Rate limiting: refill tokens and sleep if bucket is empty
            if speed_limit.is_some() {
                let now = Instant::now();
                let elapsed = now.duration_since(last_refill).as_millis() as u64;
                bucket = (bucket + elapsed * bucket_capacity / 1000).min(bucket_capacity);
                last_refill = now;

                if bucket < chunk_len {
                    // Need to wait for tokens to accumulate
                    let wait_ms = ((chunk_len - bucket) * 1000 / bucket_capacity).max(1);
                    tokio::time::sleep(Duration::from_millis(wait_ms)).await;
                    // Refill after sleep
                    let now = Instant::now();
                    let elapsed = now.duration_since(last_refill).as_millis() as u64;
                    bucket = (bucket + elapsed * bucket_capacity / 1000).min(bucket_capacity);
                    last_refill = now;
                }
                bucket = bucket.saturating_sub(chunk_len);
            }

            file.seek(std::io::SeekFrom::Start(pos))
                .await
                .map_err(|e| e.to_string())?;
            file.write_all(&bytes)
                .await
                .map_err(|e| e.to_string())?;
            file.flush().await.map_err(|e| e.to_string())?;
            pos += chunk_len;
            received = received.saturating_add(chunk_len);
            downloaded.fetch_add(chunk_len, Ordering::Relaxed);
        }

        if end != u64::MAX {
            let expected = end.saturating_sub(start).saturating_add(1);
            if received != expected {
                return Err(format!(
                    "Chunk {chunk_idx} received {received} of {expected} expected bytes"
                ));
            }
        }

        Ok(())
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

fn format_size(bytes: u64) -> String {
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

pub fn format_speed_simple(bps: u64) -> String {
    if bps < 1024 {
        format!("{}/s", bps)
    } else if bps < 1024 * 1024 {
        format!("{:.0} KB/s", bps as f64 / 1024.0)
    } else if bps < 1024 * 1024 * 1024 {
        format!("{:.0} MB/s", bps as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB/s", bps as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

// ── State Persistence ───────────────────────────────────────────────────────

/// Serializable record for a single download, saved to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub id: usize,
    pub url: String,
    pub filename: String,
    pub save_path: PathBuf,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub status: DownloadStatus,
    pub supports_range: bool,
    pub num_chunks: usize,
    #[serde(default)]
    pub expected_sha256: Option<String>,
    #[serde(default)]
    pub verify_status: VerifyStatus,
    #[serde(default)]
    pub speed_limit: Option<u64>,
    #[serde(default)]
    pub chunk_progress: Vec<u64>,
}

/// Full application state saved to disk.
#[derive(Debug, Serialize, Deserialize)]
struct AppState {
    records: Vec<DownloadRecord>,
    download_dir: PathBuf,
    next_id: usize,
}

impl App {
    /// Directory where state files are stored: ~/.yegin/
    pub fn state_dir() -> PathBuf {
        if cfg!(test) {
            std::env::temp_dir().join(format!("yegin-test-state-{}", std::process::id()))
        } else {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".yegin")
        }
    }

    fn state_file() -> PathBuf {
        Self::state_dir().join("state.json")
    }

    /// Backup an existing file before overwriting (keeps one .bak copy).
    fn backup_file(path: &Path) {
        if path.exists() {
            let bak = path.with_extension("json.bak");
            let _ = std::fs::copy(path, bak);
        }
    }

    /// Deserialize a file, falling back to .bak when the main file is absent
    /// or invalid.
    fn read_with_fallback<T: DeserializeOwned>(path: &Path) -> Option<T> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|data| serde_json::from_str(&data).ok())
            .or_else(|| {
        let bak = path.with_extension("json.bak");
                std::fs::read_to_string(bak)
                    .ok()
                    .and_then(|data| serde_json::from_str(&data).ok())
            })
    }

    /// Save current download state to disk.
    pub fn save_state(&self) {
        let dir = Self::state_dir();
        let _ = std::fs::create_dir_all(&dir);

        let records: Vec<DownloadRecord> = self
            .downloads
            .iter()
            .map(|d| {
                DownloadRecord {
                id: d.id,
                url: d.url.clone(),
                filename: d.filename.clone(),
                save_path: d.save_path.clone(),
                total_bytes: d.total_bytes,
                downloaded_bytes: d.downloaded_bytes,
                status: match &d.status {
                    DownloadStatus::Downloading => DownloadStatus::Paused,
                    _other => d.status.clone(),
                },
                supports_range: d.supports_range,
                num_chunks: d.num_chunks,
                expected_sha256: d.expected_sha256.clone(),
                verify_status: d.verify_status.clone(),
                speed_limit: d.speed_limit,
                chunk_progress: d.chunk_downloaded.clone(),
                }
            })
            .collect();

        let state = AppState {
            records,
            download_dir: self.download_dir.clone(),
            next_id: self.next_id,
        };

        if let Ok(json) = serde_json::to_string_pretty(&state) {
            Self::backup_file(&Self::state_file());
            let _ = std::fs::write(Self::state_file(), json);
        }
    }

    /// Load download state from disk and restore downloads.
    pub fn load_state(&mut self) {
        let path = Self::state_file();
        let Some(state) = Self::read_with_fallback::<AppState>(&path) else {
            return;
        };

        self.download_dir = state.download_dir;
        self.next_id = state.next_id;

        for record in state.records {
            let item = DownloadItem {
                id: record.id,
                url: record.url,
                filename: record.filename,
                save_path: record.save_path,
                total_bytes: record.total_bytes,
                downloaded_bytes: record.downloaded_bytes,
                status: record.status,
                speed: 0.0,
                start_time: None,
                elapsed: Duration::ZERO,
                error: None,
                num_chunks: record.num_chunks,
                chunk_downloaded: if !record.chunk_progress.is_empty() {
                    record.chunk_progress
                } else {
                    vec![0; record.num_chunks]
                },
                supports_range: record.supports_range,
                expected_sha256: record.expected_sha256,
                verify_status: record.verify_status,
                speed_limit: record.speed_limit,
                cancel_token: Arc::new(AtomicBool::new(false)),
                created_at: Instant::now(),
            };
            self.downloads.push(item);
        }

        if !self.downloads.is_empty() {
            self.set_status(format!("📂 Restored {} download(s)", self.downloads.len()));
        }
    }

    /// Auto-resume: mark incomplete downloads as Paused so user can resume them.
    /// downloaded_bytes is already restored from the state file — don't overwrite it.
    pub fn auto_resume_incomplete(&mut self) {
        for item in self.downloads.iter_mut() {
            if item.status == DownloadStatus::Pending || item.status == DownloadStatus::Paused {
                // Only mark as Paused if there's actual progress to resume from
                if item.downloaded_bytes > 0 && item.total_bytes.is_some_and(|t| item.downloaded_bytes < t) {
                    item.status = DownloadStatus::Paused;
                }
            }
        }
    }

    // ── History Persistence ───────────────────────────────────────────────

    fn history_file() -> PathBuf {
        Self::state_dir().join("history.json")
    }

    pub fn save_history(&self) {
        let dir = Self::state_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(json) = serde_json::to_string_pretty(&self.history) {
            Self::backup_file(&Self::history_file());
            let _ = std::fs::write(Self::history_file(), json);
        }
    }

    pub fn load_history(&mut self) {
        let path = Self::history_file();
        if let Some(mut history) = Self::read_with_fallback::<Vec<DownloadHistoryEntry>>(&path) {
                // Enforce size limit on load
                if history.len() > 100 {
                    history.truncate(100);
                }
                self.history = history;
                // Restore total bytes from history
                self.total_bytes_downloaded = self.history.iter().map(|e| e.size_bytes).sum();
        }
    }

    // ── Settings Persistence ──────────────────────────────────────────────

    fn settings_file() -> PathBuf {
        Self::state_dir().join("settings.json")
    }

    pub fn save_settings(&self) {
        let dir = Self::state_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(json) = serde_json::to_string_pretty(&self.settings) {
            Self::backup_file(&Self::settings_file());
            let _ = std::fs::write(Self::settings_file(), json);
        }
    }

    pub fn load_settings(&mut self) {
        let path = Self::settings_file();
        if let Some(settings) = Self::read_with_fallback::<DownloadSettings>(&path) {
            self.settings = settings;
        }
    }

    /// Auto-save state, history, and settings periodically during active downloads.
    /// Called from the main event loop. Saves at most once every 30 seconds.
    pub fn auto_save(&mut self) {
        let auto_save_interval = std::time::Duration::from_secs(30);
        if self.last_auto_save.elapsed() >= auto_save_interval {
            let has_active = self.downloads.iter().any(|d| {
                matches!(d.status, DownloadStatus::Downloading | DownloadStatus::Paused)
            });
            if has_active {
                self.save_state();
                self.save_history();
                self.save_settings();
                self.last_auto_save = Instant::now();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── Helper ──────────────────────────────────────────────────────────

    fn make_download_item() -> DownloadItem {
        DownloadItem::new(1, "https://example.com/test".into(), PathBuf::from("/tmp/test"))
    }

    fn make_app() -> App {
        let mut app = App::new();
        // Clear any persisted state from previous sessions
        app.downloads.clear();
        app.history.clear();
        app.selected = 0;
        app.next_id = 1;
        app
    }

    fn add_test_download(app: &mut App, url: &str) -> usize {
        app.add_download(url.to_string(), None, None);
        app.downloads.last().unwrap().id
    }

    // ── format_size ──────────────────────────────────────────────────────

    #[test]
    fn format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1), "1 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1024 * 1023), "1023.0 KB");
    }

    #[test]
    fn format_size_megabytes() {
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(1024 * 1024 * 5), "5.0 MB");
        assert_eq!(format_size(1024 * 1024 * 1023), "1023.0 MB");
    }

    #[test]
    fn format_size_gigabytes() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_size(1024 * 1024 * 1024 * 5), "5.00 GB");
    }

    #[test]
    fn format_size_boundary_1023_to_1024() {
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1.0 KB");
    }

    #[test]
    fn format_size_boundary_mb_to_gb() {
        let mb = 1024u64 * 1024;
        assert_eq!(format_size(mb * 1023), "1023.0 MB");
        assert_eq!(format_size(mb * 1024), "1.00 GB");
    }

    // ── format_speed_simple ──────────────────────────────────────────────

    #[test]
    fn format_speed_simple_bytes() {
        assert_eq!(format_speed_simple(0), "0/s");
        assert_eq!(format_speed_simple(512), "512/s");
        assert_eq!(format_speed_simple(1023), "1023/s");
    }

    #[test]
    fn format_speed_simple_kilobytes() {
        assert_eq!(format_speed_simple(1024), "1 KB/s");
        assert_eq!(format_speed_simple(1024 * 5), "5 KB/s");
    }

    #[test]
    fn format_speed_simple_megabytes() {
        assert_eq!(format_speed_simple(1024 * 1024), "1 MB/s");
        assert_eq!(format_speed_simple(1024 * 1024 * 10), "10 MB/s");
    }

    #[test]
    fn format_speed_simple_gigabytes() {
        assert_eq!(format_speed_simple(1024 * 1024 * 1024), "1.0 GB/s");
    }

    // ── filename_from_url ───────────────────────────────────────────────

    #[test]
    fn filename_from_url_simple() {
        let app = make_app();
        assert_eq!(app.filename_from_url("https://example.com/file.zip"), "file.zip");
    }

    #[test]
    fn filename_from_url_nested_path() {
        let app = make_app();
        assert_eq!(app.filename_from_url("https://cdn.example.com/path/to/document.pdf"), "document.pdf");
    }

    #[test]
    fn filename_from_url_with_query() {
        let app = make_app();
        assert_eq!(app.filename_from_url("https://example.com/file.zip?token=abc&v=2"), "file.zip");
    }

    #[test]
    fn filename_from_url_percent_encoded() {
        let app = make_app();
        assert_eq!(app.filename_from_url("https://example.com/my%20file.txt"), "my file.txt");
    }

    #[test]
    fn filename_from_url_trailing_slash() {
        let app = make_app();
        let result = app.filename_from_url("https://example.com/");
        // Should fallback to download_N
        assert!(result.starts_with("download_"));
    }

    #[test]
    fn filename_from_url_no_extension() {
        let app = make_app();
        assert_eq!(app.filename_from_url("https://example.com/readme"), "readme");
    }

    #[test]
    fn filename_from_url_invalid_url() {
        let app = make_app();
        let result = app.filename_from_url("not a url");
        assert!(result.starts_with("download_"));
    }

    #[test]
    fn filename_from_url_deeply_nested() {
        let app = make_app();
        assert_eq!(
            app.filename_from_url("https://a.com/b/c/d/e/f.tar.gz"),
            "f.tar.gz"
        );
    }

    // ── DownloadItem::progress ──────────────────────────────────────────

    #[test]
    fn progress_known_total() {
        let mut dl = make_download_item();
        dl.total_bytes = Some(1000);
        dl.downloaded_bytes = 500;
        assert_eq!(dl.progress(), 50.0);
    }

    #[test]
    fn progress_complete() {
        let mut dl = make_download_item();
        dl.total_bytes = Some(1000);
        dl.downloaded_bytes = 1000;
        assert_eq!(dl.progress(), 100.0);
    }

    #[test]
    fn progress_zero_total() {
        let mut dl = make_download_item();
        dl.total_bytes = Some(0);
        dl.downloaded_bytes = 0;
        assert_eq!(dl.progress(), 0.0);
    }

    #[test]
    fn progress_no_total() {
        let dl = make_download_item();
        assert_eq!(dl.progress(), 0.0);
    }

    #[test]
    fn progress_clamped_at_100() {
        let mut dl = make_download_item();
        dl.total_bytes = Some(100);
        dl.downloaded_bytes = 200;
        assert_eq!(dl.progress(), 100.0);
    }

    #[test]
    fn progress_one_percent() {
        let mut dl = make_download_item();
        dl.total_bytes = Some(1000);
        dl.downloaded_bytes = 10;
        assert!((dl.progress() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn progress_ninety_nine_percent() {
        let mut dl = make_download_item();
        dl.total_bytes = Some(1000);
        dl.downloaded_bytes = 999;
        assert!((dl.progress() - 99.9).abs() < 0.01);
    }

    // ── DownloadItem::has_total ─────────────────────────────────────────

    #[test]
    fn has_total_true() {
        let mut dl = make_download_item();
        dl.total_bytes = Some(1024);
        assert!(dl.has_total());
    }

    #[test]
    fn has_total_false_none() {
        let dl = make_download_item();
        assert!(!dl.has_total());
    }

    #[test]
    fn has_total_false_zero() {
        let mut dl = make_download_item();
        dl.total_bytes = Some(0);
        assert!(!dl.has_total());
    }

    // ── DownloadItem::chunk_progress ────────────────────────────────────

    #[test]
    fn chunk_progress_out_of_bounds() {
        let dl = make_download_item();
        assert_eq!(dl.chunk_progress(0), 0.0);
        assert_eq!(dl.chunk_progress(5), 0.0);
    }

    #[test]
    fn chunk_progress_single_chunk() {
        let mut dl = make_download_item();
        dl.num_chunks = 1;
        dl.chunk_downloaded = vec![500];
        dl.total_bytes = Some(1000);
        assert_eq!(dl.chunk_progress(0), 0.0);
    }

    #[test]
    fn chunk_progress_multi_chunk() {
        let mut dl = make_download_item();
        dl.num_chunks = 4;
        dl.total_bytes = Some(1000);
        dl.chunk_downloaded = vec![250, 250, 125, 0];
        assert_eq!(dl.chunk_progress(0), 100.0);
        assert_eq!(dl.chunk_progress(1), 100.0);
        assert_eq!(dl.chunk_progress(2), 50.0);
        assert_eq!(dl.chunk_progress(3), 0.0);
    }

    #[test]
    fn chunk_progress_no_total() {
        let mut dl = make_download_item();
        dl.num_chunks = 4;
        dl.chunk_downloaded = vec![100, 200, 300, 400];
        assert_eq!(dl.chunk_progress(0), 0.0);
    }

    #[test]
    fn chunk_progress_last_chunk_uneven() {
        let mut dl = make_download_item();
        dl.num_chunks = 3;
        dl.total_bytes = Some(1000);
        dl.chunk_downloaded = vec![334, 333, 333];
        // chunk_size = 1000/3 = 333; last chunk = 1000 - 333*2 = 334
        // chunk 0: 334/333 = 100.3% clamped to 100
        // chunk 1: 333/333 = 100%
        // chunk 2: 333/334 = 99.7%
        assert_eq!(dl.chunk_progress(1), 100.0);
    }

    // ── DownloadItem::format_downloaded ─────────────────────────────────

    #[test]
    fn format_downloaded_zero() {
        let dl = make_download_item();
        assert_eq!(dl.format_downloaded(), "0 B");
    }

    #[test]
    fn format_downloaded_with_bytes() {
        let mut dl = make_download_item();
        dl.downloaded_bytes = 1024 * 5;
        assert_eq!(dl.format_downloaded(), "5.0 KB");
    }

    #[test]
    fn format_downloaded_megabytes() {
        let mut dl = make_download_item();
        dl.downloaded_bytes = 1024 * 1024 * 100;
        assert_eq!(dl.format_downloaded(), "100.0 MB");
    }

    // ── DownloadItem::new ───────────────────────────────────────────────

    #[test]
    fn new_download_item_defaults() {
        let dl = DownloadItem::new(1, "https://example.com/file.zip".into(), PathBuf::from("/tmp/file.zip"));
        assert_eq!(dl.id, 1);
        assert_eq!(dl.url, "https://example.com/file.zip");
        assert_eq!(dl.filename, "file.zip");
        assert_eq!(dl.total_bytes, None);
        assert_eq!(dl.downloaded_bytes, 0);
        assert_eq!(dl.status, DownloadStatus::Pending);
        assert_eq!(dl.speed, 0.0);
        assert_eq!(dl.num_chunks, 1);
        assert!(!dl.supports_range);
        assert_eq!(dl.verify_status, VerifyStatus::NotChecked);
        assert!(dl.speed_limit.is_none());
    }

    #[test]
    fn new_download_item_fallback_filename() {
        // PathBuf::from("/tmp") has file_name() = Some("tmp"), so no fallback
        let dl = DownloadItem::new(42, "https://example.com/".into(), PathBuf::from("/"));
        assert_eq!(dl.filename, "download_42");
    }

    // ── App::add_download ───────────────────────────────────────────────

    #[test]
    fn add_download_basic() {
        let mut app = make_app();
        app.add_download("https://example.com/file.zip".into(), None, None);
        assert_eq!(app.downloads.len(), 1);
        assert_eq!(app.downloads[0].url, "https://example.com/file.zip");
        assert_eq!(app.downloads[0].filename, "file.zip");
        assert_eq!(app.downloads[0].status, DownloadStatus::Pending);
    }

    #[test]
    fn add_download_with_custom_filename() {
        let mut app = make_app();
        app.add_download(
            "https://example.com/original.zip".into(),
            Some("custom.zip".into()),
            None,
        );
        assert_eq!(app.downloads[0].filename, "custom.zip");
    }

    #[test]
    fn add_download_with_sha256() {
        let mut app = make_app();
        app.add_download(
            "https://example.com/file.zip".into(),
            None,
            Some("abc123".into()),
        );
        assert_eq!(app.downloads[0].expected_sha256, Some("abc123".into()));
        assert_eq!(app.downloads[0].verify_status, VerifyStatus::NotChecked);
    }

    #[test]
    fn add_download_empty_sha256_ignored() {
        let mut app = make_app();
        app.add_download(
            "https://example.com/file.zip".into(),
            None,
            Some("".into()),
        );
        assert!(app.downloads[0].expected_sha256.is_none());
        assert_eq!(app.downloads[0].verify_status, VerifyStatus::NoHash);
    }

    #[test]
    fn add_download_increments_id() {
        let mut app = make_app();
        app.add_download("https://a.com/1".into(), None, None);
        app.add_download("https://a.com/2".into(), None, None);
        app.add_download("https://a.com/3".into(), None, None);
        assert_eq!(app.downloads[0].id, 1);
        assert_eq!(app.downloads[1].id, 2);
        assert_eq!(app.downloads[2].id, 3);
    }

    #[test]
    fn add_download_applies_default_speed_limit() {
        let mut app = make_app();
        app.settings.default_speed_limit = Some(1024 * 100);
        app.add_download("https://a.com/file".into(), None, None);
        assert_eq!(app.downloads[0].speed_limit, Some(1024 * 100));
    }

    // ── App::pause_download / resume_download ───────────────────────────

    #[test]
    fn pause_downloading_item() {
        let mut app = make_app();
        let id = add_test_download(&mut app, "https://a.com/file");
        // Force status to Downloading (normally done by start_download which needs network)
        app.downloads[0].status = DownloadStatus::Downloading;
        app.downloads[0].start_time = Some(Instant::now());

        app.pause_download(id);
        assert_eq!(app.downloads[0].status, DownloadStatus::Paused);
        assert_eq!(app.downloads[0].speed, 0.0);
    }

    #[test]
    fn pause_pending_item_does_nothing() {
        let mut app = make_app();
        let id = add_test_download(&mut app, "https://a.com/file");
        assert_eq!(app.downloads[0].status, DownloadStatus::Pending);
        app.pause_download(id);
        assert_eq!(app.downloads[0].status, DownloadStatus::Pending);
    }

    #[test]
    fn pause_nonexistent_id() {
        let mut app = make_app();
        app.pause_download(999);
    }

    // ── App::retry_download ─────────────────────────────────────────────

    #[tokio::test]
    async fn retry_failed_item() {
        let mut app = make_app();
        let id = add_test_download(&mut app, "https://a.com/file");
        app.downloads[0].status = DownloadStatus::Failed("timeout".into());
        app.downloads[0].downloaded_bytes = 500;
        app.downloads[0].total_bytes = Some(1000);
        app.downloads[0].speed = 100.0;
        app.downloads[0].error = Some("timeout".into());

        app.retry_download(id);
        assert_eq!(app.downloads[0].downloaded_bytes, 0);
        assert_eq!(app.downloads[0].total_bytes, None);
        assert_eq!(app.downloads[0].speed, 0.0);
        assert!(app.downloads[0].error.is_none());
        // retry calls start_download which transitions Pending -> Downloading
        assert!(app.downloads[0].status == DownloadStatus::Downloading || app.downloads[0].status == DownloadStatus::Pending);
    }

    #[test]
    fn retry_nonexistent_id() {
        let mut app = make_app();
        app.retry_download(999);
    }

    // ── App::remove_download ─────────────────────────────────────────────

    #[test]
    fn remove_download_basic() {
        let mut app = make_app();
        let id = add_test_download(&mut app, "https://a.com/file");
        assert_eq!(app.downloads.len(), 1);
        app.remove_download(id);
        assert!(app.downloads.is_empty());
    }

    #[test]
    fn remove_download_adjusts_selection() {
        let mut app = make_app();
        add_test_download(&mut app, "https://a.com/1");
        add_test_download(&mut app, "https://a.com/2");
        add_test_download(&mut app, "https://a.com/3");
        app.selected = 2; // pointing to 3rd item
        app.remove_download(3);
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn remove_download_nonexistent_id() {
        let mut app = make_app();
        add_test_download(&mut app, "https://a.com/file");
        app.remove_download(999);
        assert_eq!(app.downloads.len(), 1);
    }

    // ── App::clear_completed ─────────────────────────────────────────────

    #[test]
    fn clear_completed_removes_only_terminal() {
        let mut app = make_app();
        add_test_download(&mut app, "https://a.com/active");
        add_test_download(&mut app, "https://a.com/done");
        add_test_download(&mut app, "https://a.com/failed");
        add_test_download(&mut app, "https://a.com/paused");
        app.downloads[1].status = DownloadStatus::Completed;
        app.downloads[2].status = DownloadStatus::Failed("err".into());
        app.downloads[3].status = DownloadStatus::Paused;

        app.clear_completed();
        assert_eq!(app.downloads.len(), 2);
        assert_eq!(app.downloads[0].filename, "active");
        assert_eq!(app.downloads[1].filename, "paused");
    }

    #[test]
    fn clear_completed_empty_list() {
        let mut app = make_app();
        app.clear_completed();
    }

    #[test]
    fn clear_completed_nothing_to_clear() {
        let mut app = make_app();
        add_test_download(&mut app, "https://a.com/file");
        app.clear_completed();
        assert_eq!(app.downloads.len(), 1);
    }

    // ── App::set_speed_limit ────────────────────────────────────────────

    #[test]
    fn set_speed_limit_sets_value() {
        let mut app = make_app();
        let id = add_test_download(&mut app, "https://a.com/file");
        app.set_speed_limit(id, 1024 * 100);
        assert_eq!(app.downloads[0].speed_limit, Some(1024 * 100));
    }

    #[test]
    fn set_speed_limit_zero_clears() {
        let mut app = make_app();
        let id = add_test_download(&mut app, "https://a.com/file");
        app.downloads[0].speed_limit = Some(5000);
        app.set_speed_limit(id, 0);
        assert!(app.downloads[0].speed_limit.is_none());
    }

    #[test]
    fn set_speed_limit_nonexistent_id() {
        let mut app = make_app();
        app.set_speed_limit(999, 1000);
    }

    // ── App::set_max_concurrent ─────────────────────────────────────────

    #[test]
    fn set_max_concurrent_normal() {
        let mut app = make_app();
        app.set_max_concurrent(5);
        assert_eq!(app.max_concurrent, 5);
    }

    #[test]
    fn set_max_concurrent_clamps_below() {
        let mut app = make_app();
        app.set_max_concurrent(0);
        assert_eq!(app.max_concurrent, 1);
    }

    #[test]
    fn set_max_concurrent_clamps_above() {
        let mut app = make_app();
        app.set_max_concurrent(100);
        assert_eq!(app.max_concurrent, 10);
    }

    // ── App::process_queue ──────────────────────────────────────────────

    #[tokio::test]
    async fn process_queue_starts_pending_when_slots_available() {
        let mut app = make_app();
        app.max_concurrent = 3;
        add_test_download(&mut app, "https://a.com/1");
        add_test_download(&mut app, "https://a.com/2");
        app.process_queue();
        let downloading = app.downloads.iter().filter(|d| d.status == DownloadStatus::Downloading).count();
        assert!(downloading > 0);
    }

    #[test]
    fn process_queue_respects_concurrent_limit() {
        let mut app = make_app();
        app.max_concurrent = 1;
        add_test_download(&mut app, "https://a.com/1");
        add_test_download(&mut app, "https://a.com/2");
        add_test_download(&mut app, "https://a.com/3");
        app.downloads[0].status = DownloadStatus::Downloading;
        // 1 active, limit 1 — process_queue should not start more
        app.process_queue();
        assert_eq!(app.downloads[1].status, DownloadStatus::Pending);
        assert_eq!(app.downloads[2].status, DownloadStatus::Pending);
    }

    // ── App::move_selection ─────────────────────────────────────────────

    #[test]
    fn move_selection_up_from_zero() {
        let mut app = make_app();
        app.selected = 0;
        app.move_selection_up();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn move_selection_up_normal() {
        let mut app = make_app();
        add_test_download(&mut app, "https://a.com/1");
        add_test_download(&mut app, "https://a.com/2");
        app.selected = 1;
        app.move_selection_up();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn move_selection_down_at_end() {
        let mut app = make_app();
        add_test_download(&mut app, "https://a.com/1");
        app.selected = 0;
        app.move_selection_down();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn move_selection_down_normal() {
        let mut app = make_app();
        add_test_download(&mut app, "https://a.com/1");
        add_test_download(&mut app, "https://a.com/2");
        app.move_selection_down();
        assert_eq!(app.selected, 1);
    }

    // ── App::set_status ─────────────────────────────────────────────────

    #[test]
    fn set_status_sets_message() {
        let mut app = make_app();
        app.set_status("test message".into());
        assert_eq!(app.status_message, Some("test message".into()));
    }

    #[test]
    fn set_status_overwrites() {
        let mut app = make_app();
        app.set_status("first".into());
        app.set_status("second".into());
        assert_eq!(app.status_message, Some("second".into()));
    }

    // ── App state persistence roundtrip ─────────────────────────────────

    #[test]
    fn state_roundtrip_basic() {
        let mut app = make_app();
        app.download_dir = PathBuf::from("/tmp/yegin_test");
        app.add_download("https://example.com/file.zip".into(), Some("test.zip".into()), None);
        app.downloads[0].total_bytes = Some(1024 * 100);
        app.downloads[0].downloaded_bytes = 5000;
        app.downloads[0].status = DownloadStatus::Completed;

        // Save state
        app.save_state();

        // Load into fresh app
        let mut app2 = make_app();
        app2.load_state();

        assert_eq!(app2.downloads.len(), 1);
        assert_eq!(app2.downloads[0].url, "https://example.com/file.zip");
        assert_eq!(app2.downloads[0].filename, "test.zip");
        assert_eq!(app2.downloads[0].total_bytes, Some(1024 * 100));
        assert_eq!(app2.downloads[0].downloaded_bytes, 5000);
        assert_eq!(app2.downloads[0].status, DownloadStatus::Completed);
    }

    #[test]
    fn tests_use_an_isolated_state_directory() {
        assert_ne!(App::state_dir(), dirs::home_dir().unwrap().join(".yegin"));
    }

    #[test]
    fn add_download_keeps_untrusted_names_inside_download_dir() {
        let mut app = make_app();
        app.download_dir = PathBuf::from("/tmp/yegin-downloads");

        app.add_download("https://example.com/file".into(), Some("../../outside.bin".into()), None);
        assert_eq!(app.downloads[0].save_path, PathBuf::from("/tmp/yegin-downloads/outside.bin"));

        app.add_download("https://example.com/file".into(), Some("/etc/passwd".into()), None);
        assert_eq!(app.downloads[1].save_path, PathBuf::from("/tmp/yegin-downloads/passwd"));
    }

    #[test]
    fn metadata_does_not_change_a_running_download_path() {
        let mut app = make_app();
        let id = add_test_download(&mut app, "https://example.com/file");
        let original_path = app.downloads[0].save_path.clone();

        app.update_downloads_from_event(DownloadEvent::Metadata {
            id,
            filename: "server-name.zip".into(),
            total: Some(42),
        });

        assert_eq!(app.downloads[0].save_path, original_path);
        assert_eq!(app.downloads[0].filename, "file");
        assert_eq!(app.downloads[0].total_bytes, Some(42));
    }

    #[test]
    fn browser_directory_selection_updates_persistent_settings() {
        let mut app = make_app();
        app.browser_path = PathBuf::from("/tmp/yegin-browser-choice");
        app.browser_select_current();
        assert_eq!(app.download_dir, app.settings.download_dir);
    }

    #[test]
    fn completion_records_active_elapsed_time_and_auto_verifies() {
        let mut app = make_app();
        let path = std::env::temp_dir().join(format!("yegin-verify-{}", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        let id = add_test_download(&mut app, "https://example.com/file");
        let item = &mut app.downloads[0];
        item.save_path = path.clone();
        item.downloaded_bytes = 3;
        item.status = DownloadStatus::Downloading;
        item.start_time = Some(Instant::now() - Duration::from_secs(2));
        item.expected_sha256 = Some(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
        );

        app.update_downloads_from_event(DownloadEvent::Completed { id });

        assert!(app.history[0].duration_secs >= 2);
        assert_eq!(app.downloads[0].verify_status, VerifyStatus::Pass);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rename_download_moves_the_file_and_rejects_active_downloads() {
        let mut app = make_app();
        let directory = std::env::temp_dir().join(format!("yegin-rename-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let old_path = directory.join("old.txt");
        std::fs::write(&old_path, b"contents").unwrap();
        let id = add_test_download(&mut app, "https://example.com/file");
        app.downloads[0].save_path = old_path.clone();
        app.downloads[0].filename = "old.txt".into();

        assert_eq!(app.rename_download(id, "new.txt").unwrap(), "new.txt");
        assert!(!old_path.exists());
        assert!(directory.join("new.txt").exists());

        app.downloads[0].status = DownloadStatus::Downloading;
        assert!(app.rename_download(id, "later.txt").is_err());
        let _ = std::fs::remove_file(directory.join("new.txt"));
        let _ = std::fs::remove_dir(directory);
    }

    // ── App::verify_download ─────────────────────────────────────────────

    #[test]
    fn verify_not_completed_sets_fail() {
        let mut app = make_app();
        let id = add_test_download(&mut app, "https://a.com/file");
        app.downloads[0].expected_sha256 = Some("abc".into());
        app.verify_download(id);
        assert_eq!(app.downloads[0].verify_status, VerifyStatus::Fail);
    }

    #[test]
    fn verify_no_hash_sets_no_hash() {
        let mut app = make_app();
        let id = add_test_download(&mut app, "https://a.com/file");
        app.downloads[0].status = DownloadStatus::Completed;
        app.downloads[0].expected_sha256 = None;
        app.verify_download(id);
        assert_eq!(app.downloads[0].verify_status, VerifyStatus::NoHash);
    }

    #[test]
    fn verify_nonexistent_id() {
        let mut app = make_app();
        app.verify_download(999);
    }

    #[test]
    fn verify_missing_file_sets_fail() {
        let mut app = make_app();
        let id = add_test_download(&mut app, "https://a.com/file");
        app.downloads[0].status = DownloadStatus::Completed;
        app.downloads[0].expected_sha256 = Some("abc".into());
        app.downloads[0].save_path = PathBuf::from("/nonexistent/path/file.zip");
        app.verify_download(id);
        assert_eq!(app.downloads[0].verify_status, VerifyStatus::Fail);
    }

    // ── DownloadItem status equality ─────────────────────────────────────

    #[test]
    fn download_status_equality() {
        assert_eq!(DownloadStatus::Pending, DownloadStatus::Pending);
        assert_eq!(DownloadStatus::Downloading, DownloadStatus::Downloading);
        assert_eq!(DownloadStatus::Paused, DownloadStatus::Paused);
        assert_eq!(DownloadStatus::Completed, DownloadStatus::Completed);
        assert_eq!(DownloadStatus::Failed("x".into()), DownloadStatus::Failed("x".into()));
        assert_ne!(DownloadStatus::Pending, DownloadStatus::Downloading);
        assert_ne!(DownloadStatus::Completed, DownloadStatus::Failed("".into()));
    }

    #[test]
    fn verify_status_equality() {
        assert_eq!(VerifyStatus::NotChecked, VerifyStatus::NotChecked);
        assert_eq!(VerifyStatus::Pass, VerifyStatus::Pass);
        assert_eq!(VerifyStatus::Fail, VerifyStatus::Fail);
        assert_eq!(VerifyStatus::NoHash, VerifyStatus::NoHash);
        assert_ne!(VerifyStatus::Pass, VerifyStatus::Fail);
    }
}
