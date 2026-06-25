//! Shared rendering helpers: Catppuccin Mocha semantic colors and a
//! deterministic per-author color picked from the tailwind palette.

pub mod chat;
pub mod room_list;
pub mod status_bar;

use ratatui::style::palette::tailwind;
use ratatui::style::Color;

// Catppuccin Mocha semantic tokens (RGB literals: `ratatui-themes` pulls an
// incompatible ratatui 0.30, so we define our own against ratatui 0.29).
pub const BASE: Color = Color::Rgb(30, 30, 46);
pub const SURFACE0: Color = Color::Rgb(49, 50, 68);
pub const TEXT: Color = Color::Rgb(205, 214, 244);
pub const SUBTEXT: Color = Color::Rgb(166, 173, 200);
pub const OVERLAY: Color = Color::Rgb(108, 112, 134);
pub const MAUVE: Color = Color::Rgb(203, 166, 247);
pub const GREEN: Color = Color::Rgb(166, 227, 161);
pub const RED: Color = Color::Rgb(243, 139, 168);
pub const YELLOW: Color = Color::Rgb(249, 226, 175);

// Per-author nickname palette — tailwind c400 shades.
const AUTHOR_PALETTE: [Color; 8] = [
    tailwind::RED.c400,
    tailwind::ORANGE.c400,
    tailwind::YELLOW.c400,
    tailwind::GREEN.c400,
    tailwind::TEAL.c400,
    tailwind::BLUE.c400,
    tailwind::VIOLET.c400,
    tailwind::PINK.c400,
];

/// Deterministically map an author pubkey (hex) to a stable color via FNV-1a.
#[must_use]
pub fn author_color(pubkey: &str) -> Color {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in pubkey.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    AUTHOR_PALETTE[(hash % AUTHOR_PALETTE.len() as u64) as usize]
}
