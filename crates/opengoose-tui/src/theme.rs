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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bar_style() {
        let s = bar();
        assert_eq!(s.bg, Some(SURFACE));
        assert_eq!(s.fg, Some(TEXT));
    }

    #[test]
    fn test_title_style() {
        let s = title();
        assert_eq!(s.fg, Some(ACCENT));
        assert!(s.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_key_hint_style() {
        let s = key_hint();
        assert_eq!(s.fg, Some(ACCENT));
        assert!(s.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_border_active() {
        let s = border(true);
        assert_eq!(s.fg, Some(BORDER_ACTIVE));
    }

    #[test]
    fn test_border_inactive() {
        let s = border(false);
        assert_eq!(s.fg, Some(BORDER));
    }

    #[test]
    fn test_muted_style() {
        let s = muted();
        assert_eq!(s.fg, Some(TEXT_MUTED));
    }

    #[test]
    fn test_subtle_style() {
        let s = subtle();
        assert_eq!(s.fg, Some(TEXT_SUBTLE));
    }

    #[test]
    fn test_color_constants_are_rgb() {
        // Verify all colors are true-color RGB
        assert!(matches!(SURFACE, Color::Rgb(_, _, _)));
        assert!(matches!(TEXT, Color::Rgb(_, _, _)));
        assert!(matches!(TEXT_MUTED, Color::Rgb(_, _, _)));
        assert!(matches!(ACCENT, Color::Rgb(_, _, _)));
        assert!(matches!(SECONDARY, Color::Rgb(_, _, _)));
        assert!(matches!(SUCCESS, Color::Rgb(_, _, _)));
        assert!(matches!(ERROR, Color::Rgb(_, _, _)));
        assert!(matches!(BORDER, Color::Rgb(_, _, _)));
    }
}
