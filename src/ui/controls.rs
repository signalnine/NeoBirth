//! Keypad input handling for pattern editing.
//!
//! The NeoTrellis M4 has a 4x8 button grid, mapped as follows:
//!
//! - Row 0: Note selection (cycle through notes for each step column)
//! - Row 1: Accent toggle
//! - Row 2: Slide toggle
//! - Row 3: Utility (col 0 = clear step)

#![deny(missing_docs)]

use crate::sequencer::pattern::Step;

/// Previous button state for edge detection.
pub struct KeypadState {
    /// Previous frame's button states, indexed as `[row][col]`.
    prev: [[bool; 8]; 4],
}

/// An action resulting from a keypress.
pub enum KeyAction {
    /// Cycle the note value for step N.
    CycleNote(u8),
    /// Toggle accent on step N.
    ToggleAccent(u8),
    /// Toggle slide on step N.
    ToggleSlide(u8),
    /// Clear step N.
    ClearStep(u8),
}

impl KeypadState {
    /// Create a new `KeypadState` with all buttons released.
    pub const fn new() -> Self {
        Self {
            prev: [[false; 8]; 4],
        }
    }

    /// Scan the current button state and detect rising edges.
    ///
    /// Returns the first action found for a newly pressed button,
    /// or `None` if no new presses occurred this scan.
    pub fn scan(&mut self, pressed: &[[bool; 8]; 4]) -> Option<KeyAction> {
        let mut result = None;

        for (row, row_pressed) in pressed.iter().enumerate() {
            for (col, &is_pressed) in row_pressed.iter().enumerate() {
                let rising = is_pressed && !self.prev[row][col];
                if rising && result.is_none() {
                    result = match row {
                        0 => Some(KeyAction::CycleNote(col as u8)),
                        1 => Some(KeyAction::ToggleAccent(col as u8)),
                        2 => Some(KeyAction::ToggleSlide(col as u8)),
                        3 => {
                            // Only col 0 is mapped (clear step)
                            if col == 0 {
                                Some(KeyAction::ClearStep(col as u8))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                }
                self.prev[row][col] = is_pressed;
            }
        }

        result
    }
}

/// Apply a key action to a sequencer step.
///
/// - `CycleNote`: if the step is a rest, activate it with note 0;
///   otherwise increment the note value modulo 12.
/// - `ToggleAccent`: flip the accent flag.
/// - `ToggleSlide`: flip the slide flag.
/// - `ClearStep`: reset the step to a default rest.
pub fn apply_action(step: &mut Step, action: &KeyAction) {
    match action {
        KeyAction::CycleNote(_) => {
            if step.rest {
                step.rest = false;
                step.note = 0;
            } else {
                step.note = (step.note + 1) % 12;
            }
        }
        KeyAction::ToggleAccent(_) => {
            step.accent = !step.accent;
        }
        KeyAction::ToggleSlide(_) => {
            step.slide = !step.slide;
        }
        KeyAction::ClearStep(_) => {
            *step = Step::new();
        }
    }
}
