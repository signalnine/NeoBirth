//! Sequencer engine for step-based pattern playback.
//!
//! Manages pattern advancement and exposes atomic state for
//! interrupt-safe communication with the audio rendering loop.

pub mod pattern;

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// The current step index (0-7), readable from interrupt context.
pub static CURRENT_STEP: AtomicU8 = AtomicU8::new(0);

/// Set to `true` each time a new note is triggered.
pub static NOTE_TRIGGER: AtomicBool = AtomicBool::new(false);

/// The currently active pattern bank index.
pub static ACTIVE_BANK: AtomicU8 = AtomicU8::new(0);

/// Step sequencer that advances through patterns.
pub struct Sequencer {
    /// Pattern bank storage.
    pub patterns: pattern::PatternBank,
    /// Current step index within the active pattern.
    pub step: u8,
}

impl Sequencer {
    /// Create a new sequencer with empty patterns.
    pub const fn new() -> Self {
        Self {
            patterns: pattern::PatternBank::new(),
            step: 0,
        }
    }

    /// Advance to the next step, updating atomic state and returning
    /// a reference to the new current step.
    pub fn advance(&mut self) -> &pattern::Step {
        self.step = (self.step + 1) % pattern::STEPS_PER_PATTERN as u8;
        CURRENT_STEP.store(self.step, Ordering::Release);
        NOTE_TRIGGER.store(true, Ordering::Release);

        let bank = ACTIVE_BANK.load(Ordering::Acquire) as usize;
        &self.patterns.banks[bank].steps[self.step as usize]
    }

    /// Get a reference to the current step without advancing.
    pub fn current_step(&self) -> &pattern::Step {
        let bank = ACTIVE_BANK.load(Ordering::Acquire) as usize;
        &self.patterns.banks[bank].steps[self.step as usize]
    }
}
