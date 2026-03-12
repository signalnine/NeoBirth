//! Step sequencer pattern storage.
//!
//! Provides step, pattern, and pattern bank types for an acid-style
//! step sequencer with 8 steps per pattern and 2 banks.

/// Number of steps in each pattern.
pub const STEPS_PER_PATTERN: usize = 8;

/// Number of pattern banks.
pub const NUM_BANKS: usize = 2;

/// Base MIDI note number (C2).
pub const BASE_NOTE: u8 = 36;

/// Precomputed frequency table for MIDI notes 24-72 (C1-C5).
///
/// Frequencies in Hz, computed as 440 * 2^((n-69)/12).
#[rustfmt::skip]
const FREQ_TABLE: [f32; 49] = [
    // C1      C#1     D1      Eb1     E1      F1      F#1     G1      Ab1     A1      Bb1     B1
    32.703,  34.648, 36.708, 38.891, 41.203, 43.654, 46.249, 48.999, 51.913, 55.000, 58.270, 61.735,
    // C2      C#2     D2      Eb2     E2      F2      F#2     G2      Ab2     A2      Bb2     B2
    65.406,  69.296, 73.416, 77.782, 82.407, 87.307, 92.499, 97.999, 103.826, 110.000, 116.541, 123.471,
    // C3      C#3     D3      Eb3     E3      F3      F#3     G3      Ab3     A3      Bb3     B3
    130.813, 138.591, 146.832, 155.563, 164.814, 174.614, 184.997, 195.998, 207.652, 220.000, 233.082, 246.942,
    // C4      C#4     D4      Eb4     E4      F4      F#4     G4      Ab4     A4      Bb4     B4
    261.626, 277.183, 293.665, 311.127, 329.628, 349.228, 369.994, 391.995, 415.305, 440.000, 466.164, 493.883,
    // C5
    523.251,
];

/// Convert a MIDI note number to a frequency in Hz.
///
/// Notes outside the range 24-72 are clamped to the nearest boundary.
pub fn midi_to_freq(note: u8) -> f32 {
    let clamped = if note < 24 {
        24u8
    } else if note > 72 {
        72u8
    } else {
        note
    };
    FREQ_TABLE[(clamped - 24) as usize]
}

/// A single step in a sequencer pattern.
#[derive(Clone, Copy)]
pub struct Step {
    /// Note value (0-15), added to the base note.
    pub note: u8,
    /// Octave selector: 0 = -1 octave, 1 = base octave, 2 = +1 octave.
    pub octave: u8,
    /// Whether this step is accented (louder).
    pub accent: bool,
    /// Whether this step slides into the next.
    pub slide: bool,
    /// Whether this step is a rest (silence).
    pub rest: bool,
}

impl Step {
    /// Create a new step defaulting to a rest.
    pub const fn new() -> Self {
        Self {
            note: 0,
            octave: 1,
            accent: false,
            slide: false,
            rest: true,
        }
    }

    /// Create a step with the given note parameters (not a rest).
    pub const fn with_note(note: u8, octave: u8, accent: bool, slide: bool) -> Self {
        Self {
            note,
            octave,
            accent,
            slide,
            rest: false,
        }
    }

    /// Return the frequency in Hz for this step, or 0.0 if it is a rest.
    pub fn frequency(&self) -> f32 {
        if self.rest {
            return 0.0;
        }

        // Compute the octave offset: 0 -> -1, 1 -> 0, 2 -> +1
        let octave_offset: i8 = self.octave as i8 - 1;
        let midi_note =
            BASE_NOTE as i16 + self.note as i16 + (octave_offset as i16 * 12);

        // Clamp to u8 range before passing to midi_to_freq
        let clamped = if midi_note < 0 {
            0u8
        } else if midi_note > 127 {
            127u8
        } else {
            midi_note as u8
        };

        midi_to_freq(clamped)
    }
}

/// A pattern of steps for the sequencer.
#[derive(Clone, Copy)]
pub struct Pattern {
    /// The steps in this pattern.
    pub steps: [Step; STEPS_PER_PATTERN],
}

impl Pattern {
    /// Create a new empty pattern (all rests).
    pub const fn new() -> Self {
        Self {
            steps: [Step::new(); STEPS_PER_PATTERN],
        }
    }
}

/// A bank of patterns.
pub struct PatternBank {
    /// The patterns stored in this bank.
    pub banks: [Pattern; NUM_BANKS],
}

impl PatternBank {
    /// Create a new pattern bank with all empty patterns.
    pub const fn new() -> Self {
        Self {
            banks: [Pattern::new(); NUM_BANKS],
        }
    }

    /// Load a classic acid bassline demo into bank 0.
    ///
    /// Pattern: C(accent) C C(slide)->Eb rest F(accent) F(slide)->Ab
    pub fn load_demo(&mut self) {
        // Note offsets from BASE_NOTE (C2=36):
        // C  = 0
        // Eb = 3
        // F  = 5
        // Ab = 8
        let pattern = &mut self.banks[0];

        // Step 0: C with accent
        pattern.steps[0] = Step::with_note(0, 1, true, false);
        // Step 1: C
        pattern.steps[1] = Step::with_note(0, 1, false, false);
        // Step 2: C with slide (slides into Eb)
        pattern.steps[2] = Step::with_note(0, 1, false, true);
        // Step 3: Eb
        pattern.steps[3] = Step::with_note(3, 1, false, false);
        // Step 4: rest
        pattern.steps[4] = Step::new();
        // Step 5: F with accent
        pattern.steps[5] = Step::with_note(5, 1, true, false);
        // Step 6: F with slide (slides into Ab)
        pattern.steps[6] = Step::with_note(5, 1, false, true);
        // Step 7: Ab
        pattern.steps[7] = Step::with_note(8, 1, false, false);
    }
}
