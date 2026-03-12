//! LED display for sequencer state

use crate::colors;
use crate::sequencer::pattern::{Pattern, STEPS_PER_PATTERN};
use smart_leds::Color;

/// Total number of LEDs on the NeoTrellis M4
const NUM_LEDS: usize = 32;

/// Render the current sequencer state to the LED pixel buffer
pub fn render(pixels: &mut [Color; NUM_LEDS], pattern: &Pattern, current_step: u8) {
    for pixel in pixels.iter_mut() {
        *pixel = Color { r: 0, g: 0, b: 0 };
    }
    for col in 0..STEPS_PER_PATTERN {
        let step = &pattern.steps[col];
        let is_playhead = col == current_step as usize;
        if is_playhead {
            for row in 0..4 {
                pixels[row * 8 + col] = colors::WHITE;
            }
        } else if !step.rest {
            let base_color = colors::note_color(step.note);
            let brightness = if step.accent { 255u8 } else { 100u8 };
            pixels[col] = colors::dim(base_color, brightness);
            pixels[8 + col] = colors::dim(base_color, brightness);
            if step.slide {
                pixels[16 + col] = colors::dim(
                    Color {
                        r: 0x00,
                        g: 0x60,
                        b: 0x20,
                    },
                    brightness,
                );
            }
            if step.accent {
                pixels[24 + col] = Color {
                    r: 0x60,
                    g: 0x00,
                    b: 0x00,
                };
            }
        }
    }
}
