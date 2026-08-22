use ratatui::prelude::*;

// Catppuccin Mocha-inspired dark theme
pub struct Theme;

impl Theme {
    // Base colors
    pub const BG: Color = Color::Rgb(30, 30, 46);
    pub const BG_DARKER: Color = Color::Rgb(24, 24, 37);
    pub const BG_LIGHTER: Color = Color::Rgb(49, 50, 68);
    pub const BG_CARD: Color = Color::Rgb(39, 40, 55);
    pub const BG_HIGHLIGHT: Color = Color::Rgb(69, 71, 90);

    // Borders
    pub const BORDER: Color = Color::Rgb(88, 91, 112);

    // Text
    pub const TEXT: Color = Color::Rgb(205, 214, 244);
    pub const TEXT_DIM: Color = Color::Rgb(108, 113, 166);
    pub const TEXT_BRIGHT: Color = Color::Rgb(245, 224, 220);
    pub const TEXT_MUTE: Color = Color::Rgb(69, 71, 90);

    // Accent colors
    pub const BLUE: Color = Color::Rgb(137, 180, 250);
    pub const GREEN: Color = Color::Rgb(166, 227, 161);
    pub const YELLOW: Color = Color::Rgb(249, 226, 175);
    pub const RED: Color = Color::Rgb(243, 139, 168);
    pub const ORANGE: Color = Color::Rgb(250, 179, 135);
    pub const PURPLE: Color = Color::Rgb(180, 190, 254);
    pub const PINK: Color = Color::Rgb(245, 194, 231);
    pub const TEAL: Color = Color::Rgb(148, 226, 213);
    pub const MAUVE: Color = Color::Rgb(203, 166, 247);
    pub const SKY: Color = Color::Rgb(137, 220, 235);

    // UI-specific
    pub const PROGRESS_BG: Color = Color::Rgb(49, 50, 68);
    pub const HEADER_BG: Color = Color::Rgb(24, 24, 37);
    pub const MODAL_BG: Color = Color::Rgb(30, 30, 46);
    pub const SHADOW: Color = Color::Rgb(17, 17, 27);
}
