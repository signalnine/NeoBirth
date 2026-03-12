//! Color Schemes

#![allow(dead_code)]

use smart_leds::Color;

/// White
pub const WHITE: Color = Color {
    r: 0x60,
    g: 0x60,
    b: 0x60,
};

/// Yellow
pub const YELLOW: Color = Color {
    r: 0x60,
    g: 0x60,
    b: 0x00,
};

/// Orange
pub const ORANGE: Color = Color {
    r: 0x60,
    g: 0x20,
    b: 0x00,
};

/// Red
pub const RED: Color = Color {
    r: 0x60,
    g: 0x00,
    b: 0x00,
};

/// Map a note value (0-11) to a color on a warm→cool gradient
#[allow(dead_code)]
pub fn note_color(note: u8) -> Color {
    match note % 12 {
        0  => Color { r: 0x60, g: 0x00, b: 0x00 }, // C  - red
        1  => Color { r: 0x60, g: 0x10, b: 0x00 }, // C#
        2  => Color { r: 0x60, g: 0x20, b: 0x00 }, // D  - orange
        3  => Color { r: 0x60, g: 0x40, b: 0x00 }, // Eb
        4  => Color { r: 0x60, g: 0x60, b: 0x00 }, // E  - yellow
        5  => Color { r: 0x20, g: 0x60, b: 0x00 }, // F
        6  => Color { r: 0x00, g: 0x60, b: 0x00 }, // F#
        7  => Color { r: 0x00, g: 0x60, b: 0x20 }, // G
        8  => Color { r: 0x00, g: 0x60, b: 0x60 }, // Ab
        9  => Color { r: 0x00, g: 0x20, b: 0x60 }, // A
        10 => Color { r: 0x20, g: 0x00, b: 0x60 }, // Bb
        _  => Color { r: 0x40, g: 0x00, b: 0x60 }, // B
    }
}

/// Dim a color to a fraction of its brightness
#[allow(dead_code)]
pub fn dim(color: Color, factor: u8) -> Color {
    Color {
        r: ((color.r as u16 * factor as u16) >> 8) as u8,
        g: ((color.g as u16 * factor as u16) >> 8) as u8,
        b: ((color.b as u16 * factor as u16) >> 8) as u8,
    }
}
