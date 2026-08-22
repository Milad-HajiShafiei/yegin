<div align="center">

# ⚡ Yegin

**A blazing-fast, terminal-based download manager written in Rust.**

[![Rust](https://img.shields.io/badge/Rust-2021-blue?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

</div>

---

Yegin is a cross-platform TUI download manager with chunked parallel downloads, pause/resume, speed limiting, SHA-256 verification, and persistent state — all from your terminal.

## ✨ Features

- **Parallel chunked downloads** — splits large files into up to 4 chunks for maximum speed
- **Pause, resume & retry** — interrupt downloads and pick up exactly where you left off
- **Session persistence** — all downloads, history, and settings survive restarts
- **Speed limiting** — per-download bandwidth caps (`500k`, `10m`, `1g`)
- **SHA-256 verification** — verify file integrity after download
- **Concurrent downloads** — configurable limit (1–10 simultaneous downloads)
- **Clipboard paste** — paste a URL directly from your clipboard (`Ctrl+V`)
- **Directory browser** — pick your download folder interactively
- **Stats dashboard** — session metrics, download history, and completion times
- **File rename** — rename files before or after download
- **Open folder** — reveal downloaded files in your system file manager
- **Catppuccin Mocha theme** — beautiful dark UI with animated progress bars

## 📦 Installation

### From source

```bash
git clone https://github.com/your-username/yegin.git
cd yegin
cargo install --path .
```

### With cargo install from crates.io

```bash
cargo install yegin
```

### Prerequisites

- Rust 1.75+ (edition 2021)

## 🚀 Usage

```bash
yegin
```

### Adding a download

1. Press `a` to add a new download
2. Enter the URL (must start with `http://` or `https://`)
3. Optionally enter a custom filename (or press Enter to use the server's name)
4. Optionally enter a SHA-256 hash (or press Enter to skip)
5. Preview the file info and press Enter to start

Or paste a URL directly from your clipboard with `Ctrl+V`.

### Keyboard shortcuts

#### Navigation

| Key | Action |
|-----|--------|
| `↑` / `k` | Move selection up |
| `↓` / `j` | Move selection down |
| `g` / `Home` | Go to top |
| `G` / `End` | Go to bottom |
| `PgUp` / `PgDn` | Scroll by page |

#### Downloads

| Key | Action |
|-----|--------|
| `a` | Add new download |
| `s` | Start / resume selected |
| `p` | Pause selected |
| `r` | Resume paused download |
| `t` | Retry failed download |
| `d` / `Del` | Delete selected |
| `x` | Clear completed/failed |
| `R` | Rename file |
| `v` | Verify SHA-256 checksum |
| `o` | Open containing folder |
| `L` | Set speed limit |
| `M` | Set max concurrent downloads |
| `S` | Open stats dashboard |
| `:` | Open settings |
| `c` | Change download directory |
| `Ctrl+V` | Paste URL from clipboard |

#### General

| Key | Action |
|-----|--------|
| `?` | Toggle help overlay |
| `q` / `Esc` | Quit / close dialog |

## ⚙️ Settings

Open settings with `:` to configure:

| Setting | Description | Default |
|---------|-------------|---------|
| Max concurrent downloads | Number of simultaneous downloads (1–10) | 3 |
| Default speed limit | Global bandwidth cap (`0` = unlimited) | Unlimited |
| Download directory | Where files are saved | `~/Downloads` |
| Auto-verify SHA-256 | Verify checksums automatically | On |

## 📊 Stats Dashboard

Press `S` to view your session statistics:

- Total data downloaded
- Files completed
- Average download speed
- Session duration
- Full download history with sizes and durations

## 🏗️ Architecture

```
src/
├── main.rs              # Entry point, event loop, key handling
├── app.rs               # Core state, download logic, persistence
└── ui/
    ├── mod.rs           # UI module root
    ├── header.rs        # Top bar
    ├── footer.rs        # Bottom bar with shortcuts
    ├── download_list.rs # Main download list with progress bars
    ├── detail_panel.rs  # Selected download details
    ├── modals.rs        # Help, confirm, preview, settings modals
    ├── theme.rs         # Catppuccin Mocha color palette
    └── utils.rs         # Formatting and rendering helpers
```

## 📄 License

MIT License — see [LICENSE](LICENSE) for details.
