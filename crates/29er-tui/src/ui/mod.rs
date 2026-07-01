//! Shared rendering helpers: full Catppuccin Mocha palette, deterministic
//! per-author color, and time/pubkey/spinner formatters. All later waves
//! depend on the symbols declared here; do not redefine them elsewhere.
use ratatui::style::Color;

pub mod chat;
pub mod composer;
pub mod help;
pub mod login;
pub mod membership;
pub mod palette;
pub mod room_list;
pub mod smoke;
pub mod status_bar;

// Catppuccin Mocha (issue #4 spec).
pub const BASE: Color = Color::Rgb(30, 30, 46);
pub const MANTLE: Color = Color::Rgb(24, 24, 37);
pub const CRUST: Color = Color::Rgb(17, 17, 27);
pub const TEXT: Color = Color::Rgb(205, 214, 244);
pub const SUBTEXT0: Color = Color::Rgb(166, 173, 200);
pub const SUBTEXT: Color = SUBTEXT0; // alias retained for existing call sites
pub const LAVENDER: Color = Color::Rgb(180, 190, 254);
pub const MAUVE: Color = Color::Rgb(203, 166, 247);
pub const RED: Color = Color::Rgb(243, 139, 168);
pub const PEACH: Color = Color::Rgb(250, 179, 135);
pub const YELLOW: Color = Color::Rgb(249, 226, 175);
pub const GREEN: Color = Color::Rgb(166, 227, 161);
pub const TEAL: Color = Color::Rgb(148, 226, 213);
pub const BLUE: Color = Color::Rgb(137, 180, 250);
pub const SKY: Color = Color::Rgb(137, 220, 235);
pub const SURFACE0: Color = Color::Rgb(49, 50, 68);
pub const SURFACE1: Color = Color::Rgb(69, 71, 90);
pub const OVERLAY0: Color = Color::Rgb(108, 112, 134);
pub const OVERLAY: Color = OVERLAY0; // alias retained for existing call sites

// Per-author palette (issue #7 spec).
const AUTHOR_PALETTE: [Color; 8] = [BLUE, LAVENDER, MAUVE, PEACH, GREEN, TEAL, SKY, RED];

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

#[must_use]
pub fn short_pubkey(pubkey: &str) -> String {
    if pubkey.len() <= 12 {
        pubkey.to_string()
    } else {
        format!("{}\u{2026}{}", &pubkey[..8], &pubkey[pubkey.len() - 4..])
    }
}

/// HH:MM (UTC) from unix seconds, no chrono dependency.
#[must_use]
pub fn clock_time(created_at: u64) -> String {
    let secs = created_at % 86_400;
    format!("{:02}:{:02}", secs / 3600, (secs % 3600) / 60)
}

/// Short relative age ("now", "5m", "2h", "3d").
#[must_use]
pub fn relative_time(created_at: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(created_at);
    let delta = now.saturating_sub(created_at);
    if delta < 60 {
        "now".to_string()
    } else if delta < 3600 {
        format!("{}m", delta / 60)
    } else if delta < 86_400 {
        format!("{}h", delta / 3600)
    } else {
        format!("{}d", delta / 86_400)
    }
}

/// Whole-day bucket (days since the Unix epoch, UTC) used to detect day
/// boundaries between consecutive messages for Slack-style day dividers.
#[must_use]
pub fn day_index(created_at: u64) -> i64 {
    (created_at / 86_400) as i64
}

/// Human day label for a day divider: "Today" / "Yesterday" relative to the
/// current UTC day, otherwise "Sat, Jun 27 2026". Pure date math (no chrono).
#[must_use]
pub fn day_label(created_at: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(created_at);
    let today = day_index(now);
    let day = day_index(created_at);
    match today - day {
        0 => return "Today".to_string(),
        1 => return "Yesterday".to_string(),
        _ => {}
    }
    let (year, month, dom) = civil_from_days(day);
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    const WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    // 1970-01-01 was a Thursday; `day` counts days since the epoch.
    let weekday = WEEKDAYS[day.rem_euclid(7) as usize];
    let month_name = MONTHS[(month as usize).saturating_sub(1).min(11)];
    format!("{weekday}, {month_name} {dom} {year}")
}

/// Convert days-since-Unix-epoch to a `(year, month, day)` civil date.
/// Howard Hinnant's `civil_from_days` algorithm (UTC, proleptic Gregorian).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

const SPINNER: [char; 10] = [
    '\u{280b}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283c}', '\u{2834}', '\u{2826}', '\u{2827}',
    '\u{2807}', '\u{280f}',
];
#[must_use]
pub fn spinner_frame(tick: usize) -> char {
    SPINNER[tick % SPINNER.len()]
}
