use ratatui::style::{Color, Modifier, Style};

// Palette inspired by opencode (warm peach accent) + catppuccin mocha base.
// Uses true-color RGB for a refined, modern look.

// ── Base ──────────────────────────────────────────────
pub const SURFACE: Color = Color::Rgb(0x1e, 0x1e, 0x2e); // bar backgrounds

// ── Text ─────────────────────────────────────────────
pub const TEXT: Color = Color::Rgb(0xcd, 0xd6, 0xf4);
pub const TEXT_MUTED: Color = Color::Rgb(0x6c, 0x70, 0x86);
pub const TEXT_SUBTLE: Color = Color::Rgb(0x58, 0x5b, 0x70);

// ── Accent ───────────────────────────────────────────
pub const ACCENT: Color = Color::Rgb(0xfa, 0xb2, 0x83); // warm peach (opencode)
pub const SECONDARY: Color = Color::Rgb(0x89, 0xb4, 0xfa); // blue

// ── Semantic ─────────────────────────────────────────
pub const SUCCESS: Color = Color::Rgb(0xa6, 0xe3, 0xa1);
pub const ERROR: Color = Color::Rgb(0xf3, 0x8b, 0xa8);

// ── Borders ──────────────────────────────────────────
pub const BORDER: Color = Color::Rgb(0x45, 0x47, 0x5a);
pub const BORDER_ACTIVE: Color = ACCENT;

// ── Helpers ──────────────────────────────────────────

pub fn bar() -> Style {
    Style::default().bg(SURFACE).fg(TEXT)
}

pub fn title() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn key_hint() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn border(active: bool) -> Style {
    if active {
        Style::default().fg(BORDER_ACTIVE)
    } else {
        Style::default().fg(BORDER)
    }
}

pub fn muted() -> Style {
    Style::default().fg(TEXT_MUTED)
}

pub fn subtle() -> Style {
    Style::default().fg(TEXT_SUBTLE)
}
