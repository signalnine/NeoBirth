# TB-303 Acid Synth Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use conclave:executing-plans to implement this plan task-by-task.

**Goal:** Complete NeoBirth as a working TB-303-style acid house synthesizer on the Adafruit NeoTrellis M4 Express.

**Architecture:** Replace PureZen with purpose-built DSP modules (oscillator, filter, envelope) running in a timer interrupt at 22kHz. Sequencer drives note events. Main loop handles UI (keypad + accelerometer + LEDs). DAC output on PA2 (DAC0 channel).

**Tech Stack:** Rust no_std, SAMD51 PAC registers, cortex-m interrupts, micromath for float ops

**Hardware note:** DAC0 outputs on PA2 (exposed as `pins.analog` on the NeoTrellis M4), NOT the PA6/PA7 audio pins. PA2 is a header pad on the board — attach a 3.5mm jack or amplifier there.

**Verification:** No test harness exists for this no_std target. Every task verifies with `CARGO_INCREMENTAL=0 cargo build --release`. On-hardware testing requires flashing the device.

**Shared constant:** All DSP modules must use `SAMPLE_RATE` from `src/audio/mod.rs` — never define it locally. This prevents silent desynchronization if the rate changes.

---

### Task 1: Update Dependencies

**Files:**
- Modify: `Cargo.toml`

**Dependencies:** none

**Step 1: Edit Cargo.toml**

Remove `purezen` dependency. Add `micromath` and explicit `cortex-m` + `cortex-m-rt`. Update description.

```toml
[package]
name        = "neobirth"
description = "TB-303 style acid house music synthesizer for the Adafruit NeoTrellis M4 Express"
version     = "0.2.0"
authors     = ["Tony Arcieri <bascule@gmail.com>"]
license     = "Apache-2.0"
homepage    = "https://neobirth.org"
repository  = "https://github.com/NeoBirth/NeoBirth"
readme      = "README.md"
edition     = "2018"
categories  = ["embedded", "multimedia::audio", "no-std"]
keywords    = ["303", "audio", "music", "synthesis", "acid"]

[dependencies]
cortex-m = "0.5"
cortex-m-rt = "0.6"
micromath = "0.3"
panic-halt = "0.2"
trellis_m4 = { version = "0.1", features = ["adxl343", "keypad-unproven"] }
smart-leds = "0.1"
ws2812-nop-samd51 = { git = "https://github.com/smart-leds-rs/ws2812-nop-samd51.git" }

[badges]
travis-ci = { repository = "NeoBirth/NeoBirth" }

[profile.dev]
incremental = false
codegen-units = 1
debug = true
lto = false

[profile.release]
debug = true
lto = false
opt-level = "s"

[patch.crates-io]
trellis_m4 = { git = "https://github.com/atsamd-rs/atsamd" }
```

**Step 2: Verify build**

Run: `CARGO_INCREMENTAL=0 cargo build --release`
Expected: Compiles successfully (purezen removal may cause unused import warnings — that's fine, we'll clean up main.rs later)

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: replace purezen with micromath, add explicit cortex-m"
```

---

### Task 2: Create Audio DSP Modules (Pure Math, No Hardware)

**Files:**
- Create: `src/audio/mod.rs`
- Create: `src/audio/oscillator.rs`
- Create: `src/audio/filter.rs`
- Create: `src/audio/envelope.rs`

**Dependencies:** Task 1

These are pure math modules with no hardware dependencies. They operate on `f32` samples.

**Step 1: Create `src/audio/mod.rs`**

```rust
//! Audio synthesis engine
//!
//! TB-303 style signal chain: oscillator → resonant lowpass filter → envelope → output

pub mod envelope;
pub mod filter;
pub mod oscillator;

/// Audio sample rate in Hz (shared by all DSP modules)
pub const SAMPLE_RATE: f32 = 22_050.0;
```

**Step 2: Create `src/audio/oscillator.rs`**

```rust
//! Phase-accumulator oscillator with sawtooth and square waveforms

/// Waveform selection
#[derive(Clone, Copy)]
pub enum Waveform {
    /// Sawtooth wave (bright, harmonically rich)
    Saw,
    /// Square wave (hollow, odd harmonics)
    Square,
}

/// Phase-accumulator oscillator
pub struct Oscillator {
    /// Current phase [0.0, 1.0)
    phase: f32,
    /// Phase increment per sample (frequency / sample_rate)
    phase_inc: f32,
    /// Target phase increment (for slide/portamento)
    target_phase_inc: f32,
    /// Slide rate: 1.0 = instant, lower = slower glide
    slide_rate: f32,
    /// Selected waveform
    waveform: Waveform,
}

impl Oscillator {
    /// Create a new oscillator
    pub fn new() -> Self {
        Self {
            phase: 0.0,
            phase_inc: 0.0,
            target_phase_inc: 0.0,
            slide_rate: 1.0,
            waveform: Waveform::Saw,
        }
    }

    /// Set frequency in Hz (immediate, no slide)
    pub fn set_frequency(&mut self, freq: f32) {
        self.phase_inc = freq / super::SAMPLE_RATE;
        self.target_phase_inc = self.phase_inc;
    }

    /// Set frequency with slide (portamento)
    pub fn slide_to_frequency(&mut self, freq: f32) {
        self.target_phase_inc = freq / super::SAMPLE_RATE;
    }

    /// Set slide rate (0.001 = slow glide, 1.0 = instant)
    pub fn set_slide_rate(&mut self, rate: f32) {
        self.slide_rate = rate;
    }

    /// Set waveform type
    pub fn set_waveform(&mut self, waveform: Waveform) {
        self.waveform = waveform;
    }

    /// Generate next sample in [-1.0, 1.0]
    pub fn next_sample(&mut self) -> f32 {
        // Apply slide (exponential interpolation toward target)
        self.phase_inc += (self.target_phase_inc - self.phase_inc) * self.slide_rate;

        // Advance phase
        self.phase += self.phase_inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        // Generate waveform
        match self.waveform {
            Waveform::Saw => 2.0 * self.phase - 1.0,
            Waveform::Square => {
                if self.phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
        }
    }
}
```

**Step 3: Create `src/audio/filter.rs`**

```rust
//! 2-pole resonant state-variable lowpass filter
//!
//! The classic acid squelch. Self-oscillates at high resonance.

/// Resonant lowpass filter (state-variable topology)
pub struct Filter {
    /// Low-pass output state
    low: f32,
    /// Band-pass output state
    band: f32,
    /// Cutoff coefficient (derived from frequency)
    cutoff: f32,
    /// Resonance / Q factor (0.0 = none, 1.0 = self-oscillation)
    resonance: f32,
}

impl Filter {
    /// Create a new filter with default settings
    pub fn new() -> Self {
        Self {
            low: 0.0,
            band: 0.0,
            cutoff: 0.3,
            resonance: 0.5,
        }
    }

    /// Set cutoff frequency in Hz (clamped to safe range)
    pub fn set_cutoff(&mut self, freq: f32) {
        // Convert frequency to coefficient
        // f = 2 * sin(pi * freq / sample_rate)
        // Approximation valid for freq << sample_rate/2
        let f = freq / super::SAMPLE_RATE;
        // Clamp to prevent instability
        self.cutoff = if f < 0.001 {
            0.001
        } else if f > 0.45 {
            0.45
        } else {
            f
        };
    }

    /// Set resonance (0.0 = flat, 0.99 = near self-oscillation)
    pub fn set_resonance(&mut self, res: f32) {
        self.resonance = if res < 0.0 {
            0.0
        } else if res > 0.99 {
            0.99
        } else {
            res
        };
    }

    /// Process one sample through the filter, returns lowpass output
    pub fn process(&mut self, input: f32) -> f32 {
        // State-variable filter (Chamberlin)
        // q = 1.0 - resonance (damping factor)
        let q = 1.0 - self.resonance;

        // Two iterations for better stability at high frequencies
        for _ in 0..2 {
            self.low += self.cutoff * self.band;
            let high = input - self.low - q * self.band;
            self.band += self.cutoff * high;
        }

        self.low
    }

    /// Reset filter state (call on note retrigger to avoid clicks)
    pub fn reset(&mut self) {
        self.low = 0.0;
        self.band = 0.0;
    }
}
```

**Step 4: Create `src/audio/envelope.rs`**

```rust
//! Exponential decay envelope generator
//!
//! Controls both filter cutoff depth and output amplitude.
//! Accent increases peak and shortens decay for punchy 303-style hits.

/// Decay envelope generator
pub struct Envelope {
    /// Current envelope value [0.0, 1.0+] (can exceed 1.0 with accent)
    value: f32,
    /// Decay coefficient per sample (multiply each sample)
    decay: f32,
    /// Normal peak level
    peak: f32,
    /// Accented peak level (higher than normal)
    accent_peak: f32,
    /// Normal decay rate
    normal_decay: f32,
    /// Accented decay rate (faster decay)
    accent_decay: f32,
}

impl Envelope {
    /// Create a new envelope with default settings
    pub fn new() -> Self {
        // Decay time ~200ms normal, ~100ms accented
        let normal_decay = Self::decay_for_ms(200.0);
        let accent_decay = Self::decay_for_ms(100.0);

        Self {
            value: 0.0,
            decay: normal_decay,
            peak: 1.0,
            accent_peak: 1.5,
            normal_decay,
            accent_decay,
        }
    }

    /// Calculate decay coefficient for a given time in milliseconds
    fn decay_for_ms(ms: f32) -> f32 {
        let samples = ms * super::SAMPLE_RATE / 1000.0;
        // Coefficient that decays to ~1% over `samples` samples
        // 0.01 = coeff^samples → coeff = 0.01^(1/samples)
        // Approximation: 1.0 - (4.6 / samples)
        let rate = 4.6 / samples;
        1.0 - rate
    }

    /// Set decay time in milliseconds
    pub fn set_decay_ms(&mut self, ms: f32) {
        self.normal_decay = Self::decay_for_ms(ms);
        // Accent is always half the normal decay time
        self.accent_decay = Self::decay_for_ms(ms * 0.5);
    }

    /// Trigger the envelope (call on each new note)
    pub fn trigger(&mut self, accent: bool) {
        if accent {
            self.value = self.accent_peak;
            self.decay = self.accent_decay;
        } else {
            self.value = self.peak;
            self.decay = self.normal_decay;
        }
    }

    /// Get current envelope value and advance to next sample
    pub fn next_sample(&mut self) -> f32 {
        let out = self.value;
        self.value *= self.decay;
        // Kill denormals
        if self.value < 0.0001 {
            self.value = 0.0;
        }
        out
    }

    /// Get current value without advancing (for filter modulation)
    pub fn value(&self) -> f32 {
        self.value
    }
}
```

**Step 5: Wire module into main.rs**

Add `mod audio;` to `src/main.rs` (just the module declaration, don't use it yet).

**Step 6: Verify build**

Run: `CARGO_INCREMENTAL=0 cargo build --release`
Expected: Compiles. May get dead_code warnings for unused audio module — acceptable at this stage.

**Step 7: Commit**

```bash
git add src/audio/
git commit -m "feat: add audio DSP modules (oscillator, filter, envelope)"
```

---

### Task 3: DAC Driver

**Files:**
- Create: `src/audio/dac.rs`
- Modify: `src/audio/mod.rs`

**Dependencies:** Task 2

This module wraps SAMD51 DAC registers behind a safe API. It is the ONLY module allowed to use unsafe code.

**Step 1: Create `src/audio/dac.rs`**

The DAC0 output is on PA2, exposed as one of the analog pins. We access the DAC peripheral directly via the PAC.

```rust
//! SAMD51 DAC driver for audio output
//!
//! Configures DAC channel 0 (output on PA2) for direct sample writes.
//! This is the only module that uses unsafe code.

#![allow(unsafe_code)]

use trellis_m4 as hal;
use hal::target_device::DAC;

/// DAC audio output driver
pub struct Dac {
    dac: DAC,
}

impl Dac {
    /// Initialize DAC channel 0 for audio output.
    ///
    /// Caller must have already enabled the DAC clock in MCLK and configured
    /// a GCLK source for the DAC peripheral before calling this.
    pub fn new(dac: DAC) -> Self {
        // Disable DAC while configuring
        dac.ctrla.modify(|_, w| w.enable().clear_bit());
        while dac.syncbusy.read().enable().bit_is_set() {}

        // Set voltage reference to VDDANA (analog supply voltage)
        dac.ctrlb.modify(|_, w| w.refsel().vddana());

        // Configure channel 0:
        // - Enable the channel
        // - Set current control for our sample rate range
        dac.dacctrl[0].modify(|_, w| {
            w.enable().set_bit()
             .cctrl().cc100k()
        });

        // Enable DAC controller
        dac.ctrla.modify(|_, w| w.enable().set_bit());
        while dac.syncbusy.read().enable().bit_is_set() {}

        Self { dac }
    }

    /// Write a sample to DAC channel 0.
    ///
    /// Input: f32 in range [-1.0, 1.0]
    /// Output: 12-bit unsigned value written to DAC DATA register
    pub fn write_sample(&mut self, sample: f32) {
        // Convert [-1.0, 1.0] → [0, 4095]
        let clamped = if sample < -1.0 {
            -1.0
        } else if sample > 1.0 {
            1.0
        } else {
            sample
        };
        let value = ((clamped + 1.0) * 2047.5) as u16;

        self.dac.data[0].write(|w| unsafe { w.data().bits(value) });
    }
}
```

**Step 2: Update `src/audio/mod.rs`**

```rust
//! Audio synthesis engine
//!
//! TB-303 style signal chain: oscillator → resonant lowpass filter → envelope → output

pub mod dac;
pub mod envelope;
pub mod filter;
pub mod oscillator;
```

**Step 3: Verify build**

Run: `CARGO_INCREMENTAL=0 cargo build --release`
Expected: Compiles. The `#![allow(unsafe_code)]` in dac.rs overrides the crate-level deny.

Note: If the PAC API differs (field names, method signatures), check the actual generated code at:
`~/.cargo/git/checkouts/atsamd-38d323de095cc6ab/e001849/pac/atsamd51g19a/src/dac/`

**Step 4: Commit**

```bash
git add src/audio/dac.rs src/audio/mod.rs
git commit -m "feat: add DAC driver for audio output on PA2"
```

---

### Task 4: Sequencer Pattern Storage

**Files:**
- Create: `src/sequencer/mod.rs`
- Create: `src/sequencer/pattern.rs`

**Dependencies:** Task 1

**Step 1: Create `src/sequencer/pattern.rs`**

```rust
//! Step sequencer pattern storage
//!
//! Each pattern is 8 steps. Two banks (A/B) can be chained for 16-step sequences.

/// Number of steps per pattern
pub const STEPS_PER_PATTERN: usize = 8;

/// Number of pattern banks
pub const NUM_BANKS: usize = 2;

/// MIDI-style note numbers for one octave starting at C2 (bass range)
/// C2=36, C#2=37, ... B2=47
pub const BASE_NOTE: u8 = 36;

/// A single step in the sequencer
#[derive(Clone, Copy)]
pub struct Step {
    /// Note value (0-15, semitones above BASE_NOTE)
    pub note: u8,
    /// Octave offset (-1, 0, +1) stored as 0, 1, 2
    pub octave: u8,
    /// Accent flag (louder, punchier envelope)
    pub accent: bool,
    /// Slide flag (portamento to next note)
    pub slide: bool,
    /// Rest flag (step is silent)
    pub rest: bool,
}

impl Step {
    /// Create a new empty (rest) step
    pub const fn new() -> Self {
        Self {
            note: 0,
            octave: 1, // middle octave
            accent: false,
            slide: false,
            rest: true,
        }
    }

    /// Create a step with a note
    pub const fn with_note(note: u8, octave: u8, accent: bool, slide: bool) -> Self {
        Self {
            note,
            octave,
            accent,
            slide,
            rest: false,
        }
    }

    /// Convert step to frequency in Hz
    pub fn frequency(&self) -> f32 {
        if self.rest {
            return 0.0;
        }
        let midi_note = BASE_NOTE as i16
            + self.note as i16
            + (self.octave as i16 - 1) * 12;

        // f = 440 * 2^((midi - 69) / 12)
        // Use lookup or approximation since we don't have libm
        midi_to_freq(midi_note as u8)
    }
}

/// A pattern of 8 steps
#[derive(Clone, Copy)]
pub struct Pattern {
    /// The steps in this pattern
    pub steps: [Step; STEPS_PER_PATTERN],
}

impl Pattern {
    /// Create an empty pattern (all rests)
    pub const fn new() -> Self {
        Self {
            steps: [Step::new(); STEPS_PER_PATTERN],
        }
    }
}

/// All pattern banks
pub struct PatternBank {
    /// Two banks of patterns
    pub banks: [Pattern; NUM_BANKS],
}

impl PatternBank {
    /// Create empty pattern banks
    pub const fn new() -> Self {
        Self {
            banks: [Pattern::new(); NUM_BANKS],
        }
    }

    /// Load a demo pattern into bank A for testing
    pub fn load_demo(&mut self) {
        let bank = &mut self.banks[0];
        // Classic acid bassline: C C C Eb - F F Ab -
        bank.steps[0] = Step::with_note(0, 1, true, false);  // C accent
        bank.steps[1] = Step::with_note(0, 1, false, false);  // C
        bank.steps[2] = Step::with_note(0, 1, false, true);   // C slide→
        bank.steps[3] = Step::with_note(3, 1, false, false);  // Eb
        bank.steps[4] = Step::new();                            // rest
        bank.steps[5] = Step::with_note(5, 1, true, false);   // F accent
        bank.steps[6] = Step::with_note(5, 1, false, true);   // F slide→
        bank.steps[7] = Step::with_note(8, 1, false, false);  // Ab
    }
}

/// Convert MIDI note number to frequency in Hz
///
/// Uses a precomputed lookup table for the relevant bass range (C1-C4, notes 24-72).
/// Notes outside this range are clamped.
fn midi_to_freq(note: u8) -> f32 {
    // Precomputed: 440.0 * 2^((n - 69) / 12) for n = 24..72
    #[allow(clippy::excessive_precision)]
    const FREQ_TABLE: [f32; 49] = [
        32.703, 34.648, 36.708, 38.891, 41.203, 43.654, 46.249, 48.999,
        51.913, 55.000, 58.270, 61.735,  // C1-B1
        65.406, 69.296, 73.416, 77.782, 82.407, 87.307, 92.499, 97.999,
        103.83, 110.00, 116.54, 123.47,  // C2-B2
        130.81, 138.59, 146.83, 155.56, 164.81, 174.61, 185.00, 196.00,
        207.65, 220.00, 233.08, 246.94,  // C3-B3
        261.63, 277.18, 293.66, 311.13, 329.63, 349.23, 369.99, 392.00,
        415.30, 440.00, 466.16, 493.88,  // C4-B4
        523.25,                           // C5
    ];

    let clamped = if note < 24 { 24 } else if note > 72 { 72 } else { note };
    FREQ_TABLE[(clamped - 24) as usize]
}
```

**Step 2: Create `src/sequencer/mod.rs`**

```rust
//! Step sequencer engine
//!
//! Manages pattern playback, step advancement, and bank switching.

pub mod pattern;

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use pattern::{PatternBank, STEPS_PER_PATTERN};

/// Current step index (written by clock ISR, read by main loop for LEDs)
pub static CURRENT_STEP: AtomicU8 = AtomicU8::new(0);

/// Note trigger flag (set by clock, cleared by audio ISR)
pub static NOTE_TRIGGER: AtomicBool = AtomicBool::new(false);

/// Active bank index (0 or 1, set by main loop, read by clock ISR)
pub static ACTIVE_BANK: AtomicU8 = AtomicU8::new(0);

/// Sequencer state
pub struct Sequencer {
    /// Pattern data
    pub patterns: PatternBank,
    /// Current step within the active pattern
    step: u8,
}

impl Sequencer {
    /// Create a new sequencer
    pub const fn new() -> Self {
        Self {
            patterns: PatternBank::new(),
            step: 0,
        }
    }

    /// Advance to the next step. Returns the step data for the new step.
    pub fn advance(&mut self) -> &pattern::Step {
        self.step = (self.step + 1) % STEPS_PER_PATTERN as u8;
        CURRENT_STEP.store(self.step, Ordering::Relaxed);
        NOTE_TRIGGER.store(true, Ordering::Release);

        let bank = ACTIVE_BANK.load(Ordering::Relaxed) as usize;
        &self.patterns.banks[bank].steps[self.step as usize]
    }

    /// Get the current step data without advancing
    pub fn current_step(&self) -> &pattern::Step {
        let bank = ACTIVE_BANK.load(Ordering::Relaxed) as usize;
        &self.patterns.banks[bank].steps[self.step as usize]
    }
}
```

**Step 3: Add `mod sequencer;` to `src/main.rs`**

**Step 4: Verify build**

Run: `CARGO_INCREMENTAL=0 cargo build --release`
Expected: Compiles with possible dead_code warnings.

**Step 5: Commit**

```bash
git add src/sequencer/
git commit -m "feat: add sequencer pattern storage and step advancement"
```

---

### Task 5: Timer and Interrupt Infrastructure

**Files:**
- Create: `src/audio/engine.rs`
- Modify: `src/audio/mod.rs`
- Modify: `src/main.rs`

**Dependencies:** Task 2, Task 3, Task 4

This task wires everything together: configures TC0 for audio ISR, TC1 for sequencer clock, sets up shared state, and restructures main.rs.

**Step 1: Create `src/audio/engine.rs`**

This holds the global audio state accessed from the interrupt handler.

```rust
//! Audio engine — glue between DSP modules and interrupt handler
//!
//! Holds the global synth state in a static that the TC0 ISR reads/writes.

use core::sync::atomic::{AtomicU16, Ordering};
use super::oscillator::{Oscillator, Waveform};
use super::filter::Filter;
use super::envelope::Envelope;

/// Filter cutoff from accelerometer (set by main loop, read by ISR)
/// Stored as frequency in Hz (80-8000 mapped to u16)
pub static FILTER_CUTOFF: AtomicU16 = AtomicU16::new(1000);

/// Complete synth voice state
pub struct SynthVoice {
    /// Oscillator
    pub osc: Oscillator,
    /// Resonant lowpass filter
    pub filter: Filter,
    /// Amplitude/filter envelope
    pub envelope: Envelope,
}

impl SynthVoice {
    /// Create a new synth voice
    pub fn new() -> Self {
        Self {
            osc: Oscillator::new(),
            filter: Filter::new(),
            envelope: Envelope::new(),
        }
    }

    /// Trigger a new note
    pub fn note_on(&mut self, freq: f32, accent: bool, slide: bool) {
        if slide {
            self.osc.slide_to_frequency(freq);
        } else {
            self.osc.set_frequency(freq);
        }
        self.envelope.trigger(accent);
        if !slide {
            self.filter.reset();
        }
    }

    /// Generate one output sample
    pub fn render(&mut self) -> f32 {
        // Read cutoff from accelerometer (atomic)
        let base_cutoff = FILTER_CUTOFF.load(Ordering::Relaxed) as f32;

        // Envelope modulates cutoff upward
        let env = self.envelope.next_sample();
        let cutoff = base_cutoff + env * 4000.0;
        self.filter.set_cutoff(cutoff);

        // Signal chain: oscillator → filter → envelope amplitude
        let osc_out = self.osc.next_sample();
        let filtered = self.filter.process(osc_out);

        // Apply envelope as amplitude (clamp env to 1.0 for amplitude)
        let amp = if env > 1.0 { 1.0 } else { env };
        filtered * amp
    }
}
```

**Step 2: Update `src/audio/mod.rs`**

```rust
//! Audio synthesis engine
//!
//! TB-303 style signal chain: oscillator → resonant lowpass filter → envelope → output

pub mod dac;
pub mod engine;
pub mod envelope;
pub mod filter;
pub mod oscillator;
```

**Step 3: Rewrite `src/main.rs`**

This is the big integration step. Replace the current LED-only main with the full synth initialization and main loop.

```rust
//! TB-303 style acid house synthesizer for the [Adafruit NeoTrellis M4],
//! inspired by [Propellerhead ReBirth].
//!
//! [Adafruit NeoTrellis M4]: https://learn.adafruit.com/adafruit-neotrellis-m4
//! [Propellerhead ReBirth]: https://en.wikipedia.org/wiki/ReBirth_RB-338

#![no_std]
#![no_main]
#![deny(
    warnings,
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications
)]

mod audio;
mod colors;
mod sequencer;

#[allow(unused_imports)]
use panic_halt;
use trellis_m4 as hal;
use ws2812_nop_samd51 as ws2812;

use hal::prelude::*;
use hal::{clock::GenericClockController, delay::Delay, entry, CorePeripherals, Peripherals};
use smart_leds::{Color, SmartLedsWrite};

use core::cell::RefCell;
use cortex_m::interrupt::Mutex;

use audio::dac::Dac;
use audio::engine::SynthVoice;
use sequencer::Sequencer;

/// Total number of LEDs on the NeoTrellis M4
const NUM_LEDS: usize = 32;

/// Global synth voice, accessed from TC0 ISR
static SYNTH: Mutex<RefCell<Option<SynthVoice>>> = Mutex::new(RefCell::new(None));

/// Global DAC, accessed from TC0 ISR
static DAC: Mutex<RefCell<Option<Dac>>> = Mutex::new(RefCell::new(None));

/// Global sequencer, accessed from main loop and TC1 ISR
static SEQUENCER: Mutex<RefCell<Option<Sequencer>>> = Mutex::new(RefCell::new(None));

/// Main entrypoint
#[entry]
fn main() -> ! {
    let mut peripherals = Peripherals::take().unwrap();
    let core_peripherals = CorePeripherals::take().unwrap();

    let mut clocks = GenericClockController::with_internal_32kosc(
        peripherals.GCLK,
        &mut peripherals.MCLK,
        &mut peripherals.OSC32KCTRL,
        &mut peripherals.OSCCTRL,
        &mut peripherals.NVMCTRL,
    );

    let mut pins = hal::Pins::new(peripherals.PORT).split();
    let mut delay = Delay::new(core_peripherals.SYST, &mut clocks);

    // --- NeoPixels ---
    let neopixel_pin = pins.neopixel.into_push_pull_output(&mut pins.port);
    let mut neopixel = ws2812::Ws2812::new(neopixel_pin);
    let mut pixels = [Color::default(); NUM_LEDS];

    // --- Accelerometer ---
    let adxl343 = pins
        .accel
        .open(
            &mut clocks,
            peripherals.SERCOM2,
            &mut peripherals.MCLK,
            &mut pins.port,
        )
        .unwrap();
    let mut accel_tracker = adxl343.try_into_tracker().unwrap();

    // --- DAC setup ---
    // Enable DAC peripheral clock in MCLK
    peripherals.MCLK.apbdmask.modify(|_, w| w.dac_().set_bit());
    // Configure GCLK source for DAC (required — DAC will not function without this)
    let gclk0 = clocks.gclk0();
    let _dac_clock = clocks.dac(&gclk0).unwrap();
    // Configure PA2 pin for DAC0 alternate function (peripheral function B)
    let _dac_pin = pins.analog.a0.into_function_b(&mut pins.port);
    let dac = Dac::new(peripherals.DAC);

    // --- Synth voice ---
    let synth = SynthVoice::new();

    // --- Sequencer ---
    let mut seq = Sequencer::new();
    seq.patterns.load_demo();

    // Store globals for ISR access
    cortex_m::interrupt::free(|cs| {
        SYNTH.borrow(cs).replace(Some(synth));
        DAC.borrow(cs).replace(Some(dac));
        SEQUENCER.borrow(cs).replace(Some(seq));
    });

    // TODO (Task 6): Configure TC0 at 22,050Hz for audio ISR
    // TODO (Task 6): Configure TC1 at BPM rate for sequencer clock ISR
    // TODO (Task 6): Set NVIC priorities and enable interrupts

    // --- Main loop: UI polling at ~60Hz ---
    let mut reversed = false;

    loop {
        // Read accelerometer orientation
        if let Ok(orientation) = accel_tracker.orientation() {
            use hal::adxl343::accelerometer::Orientation;
            match orientation {
                Orientation::LandscapeUp => reversed = false,
                Orientation::LandscapeDown => reversed = true,
                _ => (),
            }
        }

        // Update bank based on orientation
        let bank = if reversed { 1u8 } else { 0u8 };
        sequencer::ACTIVE_BANK.store(bank, core::sync::atomic::Ordering::Relaxed);

        // TODO (Task 7): Read accelerometer raw values for filter cutoff
        // TODO (Task 8): Scan keypad for pattern editing
        // TODO (Task 9): Update LED display from sequencer state

        // Simple LED display for now: show which bank is active
        let color = if reversed { colors::RED } else { colors::WHITE };
        for pixel in pixels.iter_mut() {
            *pixel = color;
        }

        // Show current step position
        let step = sequencer::CURRENT_STEP.load(core::sync::atomic::Ordering::Relaxed) as usize;
        for row in 0..4 {
            pixels[row * 8 + step] = colors::YELLOW;
        }

        neopixel.write(pixels.iter().cloned()).unwrap();
        delay.delay_ms(16u8);
    }
}
```

**Important:** This step removes `#![deny(unsafe_code)]` from the crate-level attributes. The `dac.rs` module has its own `#![allow(unsafe_code)]`, but the crate-level deny would still block it. Instead we rely on the module-level allow.

**Step 4: Verify build**

Run: `CARGO_INCREMENTAL=0 cargo build --release`
Expected: Compiles. There will be TODO items for timer setup (Task 6).

This is the first milestone where the firmware structure matches the target architecture, even though timers aren't configured yet.

**Step 5: Commit**

```bash
git add src/
git commit -m "feat: integrate audio engine, DAC, and sequencer into main loop"
```

---

### Task 6: Timer Interrupts (Audio ISR + Sequencer Clock)

**Files:**
- Modify: `src/main.rs` (timer configuration in init, interrupt handlers)

**Dependencies:** Task 5

This is the most hardware-specific task. Timer register configuration depends on exact PAC API naming which may differ from documentation. Expect to iterate.

**Step 1: Add timer configuration to main.rs init**

After the DAC setup and before the main loop, add TC0 and TC1 configuration. Insert this code where the TODO comments are:

```rust
// --- TC0/TC1 clock source (shared) ---
// CRITICAL: TC0 and TC1 share a GCLK source. Must configure before using either timer.
let gclk0 = clocks.gclk0();
let _tc_clock = clocks.tc0_tc1(&gclk0).unwrap();

// --- TC0: Audio sample rate timer (22,050 Hz) ---
// Enable TC0 peripheral clock in MCLK
peripherals.MCLK.apbamask.modify(|_, w| w.tc0_().set_bit());

// 120MHz GCLK / prescaler / CC = 22050Hz
// With DIV16: 120MHz / 16 = 7.5MHz, CC = 7500000 / 22050 ≈ 340
let tc0 = peripherals.TC0.count16();

// Disable before configuring
tc0.ctrla.modify(|_, w| w.enable().clear_bit());
while tc0.syncbusy.read().enable().bit_is_set() {}

// 16-bit mode, prescaler DIV16, match frequency mode
tc0.ctrla.modify(|_, w| {
    w.mode().count16()
     .prescaler().div16()
});

// Set compare value for 22,050 Hz
tc0.cc[0].write(|w| unsafe { w.cc().bits(340) });

// Enable match compare 0 interrupt
tc0.intenset.write(|w| w.mc0().set_bit());

// Set waveform to MFRQ (match frequency — resets counter on CC match)
tc0.wave.write(|w| w.wavegen().mfrq());

// Enable TC0
tc0.ctrla.modify(|_, w| w.enable().set_bit());
while tc0.syncbusy.read().enable().bit_is_set() {}

// --- TC1: Sequencer clock (uses same GCLK as TC0, already configured above) ---
// 120 BPM = 2 beats/sec, 16th notes = 8 ticks/sec
// With DIV1024: 120MHz / 1024 = 117,187.5 Hz, CC = 117187 / 8 ≈ 14648
peripherals.MCLK.apbamask.modify(|_, w| w.tc1_().set_bit());

let tc1 = peripherals.TC1.count16();
tc1.ctrla.modify(|_, w| w.enable().clear_bit());
while tc1.syncbusy.read().enable().bit_is_set() {}

tc1.ctrla.modify(|_, w| {
    w.mode().count16()
     .prescaler().div1024()
});

tc1.cc[0].write(|w| unsafe { w.cc().bits(14648) });
tc1.intenset.write(|w| w.mc0().set_bit());
tc1.wave.write(|w| w.wavegen().mfrq());

tc1.ctrla.modify(|_, w| w.enable().set_bit());
while tc1.syncbusy.read().enable().bit_is_set() {}

// --- NVIC: Enable and prioritize interrupts ---
unsafe {
    core_peripherals.NVIC.set_priority(hal::target_device::Interrupt::TC0, 0); // highest
    core_peripherals.NVIC.set_priority(hal::target_device::Interrupt::TC1, 2); // lower
    cortex_m::peripheral::NVIC::unmask(hal::target_device::Interrupt::TC0);
    cortex_m::peripheral::NVIC::unmask(hal::target_device::Interrupt::TC1);
}
```

**Step 2: Add interrupt handlers at the bottom of main.rs**

```rust
use hal::target_device::interrupt;

/// Audio sample rate interrupt (22,050 Hz)
///
/// Runs the DSP chain and writes to DAC.
#[interrupt]
fn TC0() {
    // Clear interrupt flag
    let tc = unsafe { &*hal::target_device::TC0::ptr() };
    tc.count16().intflag.write(|w| w.mc0().set_bit());

    cortex_m::interrupt::free(|cs| {
        if let (Some(synth), Some(dac)) = (
            SYNTH.borrow(cs).borrow_mut().as_mut(),
            DAC.borrow(cs).borrow_mut().as_mut(),
        ) {
            // Check for new note trigger from sequencer
            if sequencer::NOTE_TRIGGER.swap(false, core::sync::atomic::Ordering::Acquire) {
                if let Some(seq) = SEQUENCER.borrow(cs).borrow().as_ref() {
                    let step = seq.current_step();
                    let freq = step.frequency();
                    if freq > 0.0 {
                        synth.note_on(freq, step.accent, step.slide);
                    }
                }
            }

            let sample = synth.render();
            dac.write_sample(sample);
        }
    });
}

/// Sequencer clock interrupt (8 Hz at 120 BPM)
///
/// Advances the sequencer to the next step.
#[interrupt]
fn TC1() {
    // Clear interrupt flag
    let tc = unsafe { &*hal::target_device::TC1::ptr() };
    tc.count16().intflag.write(|w| w.mc0().set_bit());

    cortex_m::interrupt::free(|cs| {
        if let Some(seq) = SEQUENCER.borrow(cs).borrow_mut().as_mut() {
            seq.advance();
        }
    });
}
```

**Step 3: Verify build**

Run: `CARGO_INCREMENTAL=0 cargo build --release`
Expected: Compiles. This is the **first fully functional firmware** — if flashed, it should play the demo acid pattern through the DAC output on PA2.

**Important debugging notes for hardware testing:**
- Connect an oscilloscope or headphones+amplifier to the PA2 pad
- If no sound: check GCLK configuration for DAC (may need explicit `clocks.dac()` call)
- If garbled: check TC0 compare value (target exactly 22,050 Hz)
- If too fast/slow: verify GCLK0 is actually 120MHz

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: add timer interrupts for audio ISR and sequencer clock"
```

---

### Task 7: Accelerometer Filter Control

**Files:**
- Modify: `src/main.rs` (main loop accelerometer section)

**Dependencies:** Task 6

Replace the TODO for accelerometer raw values. Use the `Accelerometer` trait to read raw X-axis tilt and map it to filter cutoff.

**Step 1: Add raw acceleration reading to main loop**

Replace the accelerometer section in the main loop with:

```rust
// Read raw accelerometer for filter cutoff
use hal::adxl343::accelerometer::Accelerometer;

// Read acceleration (returns I16x3 with x, y, z)
if let Ok(accel) = accel_tracker.acceleration() {
    // Map X-axis tilt to filter cutoff
    // Raw range: roughly -256 to +256 in ±2G mode
    // Target: 80 Hz to 8000 Hz (exponential mapping)
    let x = accel.x as f32;
    // Normalize to [0.0, 1.0]
    let normalized = (x + 256.0) / 512.0;
    let clamped = if normalized < 0.0 {
        0.0f32
    } else if normalized > 1.0 {
        1.0f32
    } else {
        normalized
    };
    // Exponential mapping: 80 * 100^clamped = 80..8000
    let cutoff = 80.0 + clamped * 7920.0;
    audio::engine::FILTER_CUTOFF.store(
        cutoff as u16,
        core::sync::atomic::Ordering::Relaxed,
    );
}
```

**Note:** The exact raw value range from the ADXL343 depends on the configured G range (default ±2G at 3.9mg/LSB → ±512 for full range). You may need to adjust the normalization constants after testing on hardware.

**Step 2: Verify build**

Run: `CARGO_INCREMENTAL=0 cargo build --release`

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: map accelerometer tilt to filter cutoff frequency"
```

---

### Task 8: Keypad Input

**Files:**
- Create: `src/ui/mod.rs`
- Create: `src/ui/controls.rs`
- Modify: `src/main.rs`

**Dependencies:** Task 6

**Step 1: Create `src/ui/mod.rs`**

```rust
//! User interface — keypad input and LED display

pub mod controls;
```

**Step 2: Create `src/ui/controls.rs`**

```rust
//! Keypad input handling for pattern editing
//!
//! 4x8 button grid mapping:
//! - Row 0: Note selection (cycle through notes for each step)
//! - Row 1: Accent toggle
//! - Row 2: Slide toggle
//! - Row 3: Utility (col 0 = clear step, col 7 = toggle waveform)

use crate::sequencer::pattern::{Step, STEPS_PER_PATTERN};

/// Previous button state for edge detection
pub struct KeypadState {
    /// Previous state of each button (true = was pressed)
    prev: [[bool; 8]; 4],
}

impl KeypadState {
    /// Create new keypad state (all buttons released)
    pub const fn new() -> Self {
        Self {
            prev: [[false; 8]; 4],
        }
    }

    /// Process a scan of the keypad matrix.
    ///
    /// `pressed[row][col]` is true if that button is currently pressed.
    /// Returns actions to apply to the pattern.
    pub fn scan(&mut self, pressed: &[[bool; 8]; 4]) -> Option<KeyAction> {
        let mut action = None;

        for row in 0..4 {
            for col in 0..8 {
                // Detect rising edge (just pressed)
                let just_pressed = pressed[row][col] && !self.prev[row][col];
                self.prev[row][col] = pressed[row][col];

                if !just_pressed {
                    continue;
                }

                match row {
                    0 => {
                        // Row 0: cycle note for this step
                        action = Some(KeyAction::CycleNote(col as u8));
                    }
                    1 => {
                        // Row 1: toggle accent
                        action = Some(KeyAction::ToggleAccent(col as u8));
                    }
                    2 => {
                        // Row 2: toggle slide
                        action = Some(KeyAction::ToggleSlide(col as u8));
                    }
                    3 => {
                        // Row 3: utility
                        if col == 0 {
                            action = Some(KeyAction::ClearStep(0));
                        }
                        // col 7 reserved for waveform toggle
                    }
                    _ => {}
                }
            }
        }

        action
    }
}

/// An action resulting from a keypress
pub enum KeyAction {
    /// Cycle the note value for step N (0-7)
    CycleNote(u8),
    /// Toggle accent on step N
    ToggleAccent(u8),
    /// Toggle slide on step N
    ToggleSlide(u8),
    /// Clear step N (set to rest)
    ClearStep(u8),
}

/// Apply a key action to a pattern step
pub fn apply_action(step: &mut Step, action: &KeyAction) {
    match action {
        KeyAction::CycleNote(s) => {
            if step.rest {
                // First press: activate step with note 0
                step.rest = false;
                step.note = 0;
            } else {
                // Subsequent presses: cycle through notes
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
```

**Step 3: Add keypad scanning to main.rs**

Add `mod ui;` declaration and keypad initialization + scanning in the main loop:

```rust
// In init section, after accelerometer:
let keypad = hal::Keypad::new(pins.keypad, &mut pins.port);
let mut keypad_state = ui::controls::KeypadState::new();

// In main loop:
// Scan keypad
let keypad_inputs = keypad.decompose();
let mut pressed = [[false; 8]; 4];
for (row_idx, row) in keypad_inputs.iter().enumerate() {
    for (col_idx, button) in row.iter().enumerate() {
        pressed[row_idx][col_idx] = button.is_low();
    }
}

if let Some(action) = keypad_state.scan(&pressed) {
    let step_idx = match &action {
        ui::controls::KeyAction::CycleNote(s) => *s,
        ui::controls::KeyAction::ToggleAccent(s) => *s,
        ui::controls::KeyAction::ToggleSlide(s) => *s,
        ui::controls::KeyAction::ClearStep(s) => *s,
    } as usize;

    cortex_m::interrupt::free(|cs| {
        if let Some(seq) = SEQUENCER.borrow(cs).borrow_mut().as_mut() {
            let bank = sequencer::ACTIVE_BANK.load(core::sync::atomic::Ordering::Relaxed) as usize;
            ui::controls::apply_action(
                &mut seq.patterns.banks[bank].steps[step_idx],
                &action,
            );
        }
    });
}
```

**Step 4: Verify build**

Run: `CARGO_INCREMENTAL=0 cargo build --release`

**Step 5: Commit**

```bash
git add src/ui/ src/main.rs
git commit -m "feat: add keypad input for pattern editing"
```

---

### Task 9: LED Sequencer Display

**Files:**
- Create: `src/ui/leds.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/colors.rs`
- Modify: `src/main.rs`

**Dependencies:** Task 8

**Step 1: Expand `src/colors.rs` with note color gradient**

Add to the existing colors module:

```rust
/// Map a note value (0-11) to a color on a warm→cool gradient
pub fn note_color(note: u8) -> Color {
    // 12-color gradient: red → orange → yellow → green → cyan → blue
    match note % 12 {
        0  => Color { r: 0x60, g: 0x00, b: 0x00 }, // C  - red
        1  => Color { r: 0x60, g: 0x10, b: 0x00 }, // C# - red-orange
        2  => Color { r: 0x60, g: 0x20, b: 0x00 }, // D  - orange
        3  => Color { r: 0x60, g: 0x40, b: 0x00 }, // Eb - amber
        4  => Color { r: 0x60, g: 0x60, b: 0x00 }, // E  - yellow
        5  => Color { r: 0x20, g: 0x60, b: 0x00 }, // F  - yellow-green
        6  => Color { r: 0x00, g: 0x60, b: 0x00 }, // F# - green
        7  => Color { r: 0x00, g: 0x60, b: 0x20 }, // G  - green-cyan
        8  => Color { r: 0x00, g: 0x60, b: 0x60 }, // Ab - cyan
        9  => Color { r: 0x00, g: 0x20, b: 0x60 }, // A  - blue
        10 => Color { r: 0x20, g: 0x00, b: 0x60 }, // Bb - indigo
        _  => Color { r: 0x40, g: 0x00, b: 0x60 }, // B  - violet
    }
}

/// Dim a color to a fraction of its brightness
pub fn dim(color: Color, factor: u8) -> Color {
    // factor: 0-255, where 255 = full brightness
    Color {
        r: ((color.r as u16 * factor as u16) >> 8) as u8,
        g: ((color.g as u16 * factor as u16) >> 8) as u8,
        b: ((color.b as u16 * factor as u16) >> 8) as u8,
    }
}
```

**Step 2: Create `src/ui/leds.rs`**

```rust
//! LED display for sequencer state
//!
//! Maps 4x8 NeoPixel grid to show pattern state:
//! - Current step column: bright white (playhead)
//! - Active notes: colored by pitch, bright if accented, dim if normal
//! - Slide indicator: row 2 lights up for steps with slide
//! - Rests: dark

use smart_leds::Color;
use crate::colors;
use crate::sequencer::pattern::{Pattern, STEPS_PER_PATTERN};

/// Number of LEDs
const NUM_LEDS: usize = 32;

/// Render the current sequencer state to the LED pixel buffer
pub fn render(
    pixels: &mut [Color; NUM_LEDS],
    pattern: &Pattern,
    current_step: u8,
) {
    // Clear all pixels
    for pixel in pixels.iter_mut() {
        *pixel = Color { r: 0, g: 0, b: 0 };
    }

    for col in 0..STEPS_PER_PATTERN {
        let step = &pattern.steps[col];
        let is_playhead = col == current_step as usize;

        if is_playhead {
            // Playhead column: bright white across all rows
            for row in 0..4 {
                pixels[row * 8 + col] = colors::WHITE;
            }
        } else if !step.rest {
            // Active step: show note info
            let base_color = colors::note_color(step.note);
            let brightness = if step.accent { 255u8 } else { 100u8 };

            // Row 0-1: note color (fills top half)
            pixels[col] = colors::dim(base_color, brightness);
            pixels[8 + col] = colors::dim(base_color, brightness);

            // Row 2: slide indicator (green tint if slide enabled)
            if step.slide {
                pixels[16 + col] = colors::dim(
                    Color { r: 0x00, g: 0x60, b: 0x20 },
                    brightness,
                );
            }

            // Row 3: accent indicator (red tint if accent)
            if step.accent {
                pixels[24 + col] = Color { r: 0x60, g: 0x00, b: 0x00 };
            }
        }
        // Rests stay dark (already cleared)
    }
}
```

**Step 3: Update `src/ui/mod.rs`**

```rust
//! User interface — keypad input and LED display

pub mod controls;
pub mod leds;
```

**Step 4: Update main loop LED section in `src/main.rs`**

Replace the simple LED display code with:

```rust
// Update LED display from sequencer state
let step_idx = sequencer::CURRENT_STEP.load(core::sync::atomic::Ordering::Relaxed);
let bank = sequencer::ACTIVE_BANK.load(core::sync::atomic::Ordering::Relaxed) as usize;

cortex_m::interrupt::free(|cs| {
    if let Some(seq) = SEQUENCER.borrow(cs).borrow().as_ref() {
        ui::leds::render(&mut pixels, &seq.patterns.banks[bank], step_idx);
    }
});

neopixel.write(pixels.iter().cloned()).unwrap();
```

**Step 5: Verify build**

Run: `CARGO_INCREMENTAL=0 cargo build --release`

**Step 6: Commit**

```bash
git add src/colors.rs src/ui/leds.rs src/ui/mod.rs src/main.rs
git commit -m "feat: add LED sequencer visualization with note colors"
```

---

### Task 10: Final Cleanup and Polish

**Files:**
- Modify: `src/main.rs` (remove dead code, clean imports)
- Modify: `README.md` (update status)
- Modify: `CLAUDE.md` (update architecture)

**Dependencies:** Task 9

**Step 1: Clean up main.rs**

- Remove any remaining `#[allow(unused_imports)]` that are no longer needed
- Remove any TODO comments that have been addressed
- Ensure all `use` statements are minimal and correct
- Verify no dead code warnings remain

**Step 2: Update README.md status section**

Replace:
```
The current code produces a working executable which can be loaded onto a
NeoTrellis M4 device, however no functionality is yet in place (unless you
like blinking LEDs).
```

With:
```
Working TB-303-style acid synthesizer. Features:
- Sawtooth/square oscillator with resonant lowpass filter
- 8-step sequencer with accent and slide (two banks for 16 steps)
- Accelerometer tilt controls filter cutoff frequency
- 4x8 keypad for live pattern editing
- NeoPixel display shows sequencer state with color-coded notes
- Audio output on PA2 (DAC0) — connect headphones or amplifier
```

**Step 3: Update CLAUDE.md architecture section**

Update to reflect the actual module structure.

**Step 4: Full verification**

```bash
CARGO_INCREMENTAL=0 cargo build --release
cargo fmt -- --check
cargo clippy
```

All three must pass.

**Step 5: Commit**

```bash
git add -A
git commit -m "chore: final cleanup, update docs for v0.2.0"
```

---

## Dependency Graph

```
Task 1 (deps) ─┬─→ Task 2 (DSP modules) ──┐
               │                            ├─→ Task 5 (integration) → Task 6 (timers) ─┬─→ Task 7 (accel)
               └─→ Task 4 (sequencer) ──────┘                                           ├─→ Task 8 (keypad) → Task 9 (LEDs)
                                                                                         └─→ Task 10 (cleanup)
               Task 3 (DAC) ────────────────────────────────────────────────────→ Task 5
```

**Parallelizable:** Tasks 2, 3, and 4 can be done in parallel after Task 1.

## Hardware Testing Checklist

After flashing each milestone:

- [ ] **After Task 6:** Probe PA2 with oscilloscope — should see ~22kHz waveform with note pattern
- [ ] **After Task 7:** Tilt device — waveform brightness/character should change (filter sweep)
- [ ] **After Task 8:** Press buttons — pattern should change audibly on next loop
- [ ] **After Task 9:** LEDs should animate showing playhead and note colors

## Known Issues to Watch For

1. **GCLK for DAC**: The DAC needs its own GCLK source configured. If `clocks.dac()` doesn't exist in this HAL version, you'll need to configure GCLK_DAC manually via the GCLK peripheral registers.

2. **TC0/TC1 GCLK**: Both timers share the TC0_TC1 clock. Must call `clocks.tc0_tc1()` to enable it before configuring the timers.

3. **WAVE register**: The `wavegen().mfrq()` call sets Match Frequency mode (counter resets on CC match). If the PAC doesn't expose this enum variant, try `wavegen().bits(1)`.

4. **Sync busy**: Always wait for `syncbusy` after modifying CTRLA on DAC and TC peripherals. Missing this causes silent configuration failures.

5. **Interrupt handler naming**: The `#[interrupt] fn TC0()` function name must exactly match the PAC's interrupt enum variant. Check `hal::target_device::Interrupt::TC0`.
