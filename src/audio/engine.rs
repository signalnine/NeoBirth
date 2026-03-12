//! Audio engine -- glue between DSP modules and interrupt handler

use super::envelope::Envelope;
use super::filter::Filter;
use super::oscillator::Oscillator;
use core::sync::atomic::{AtomicU16, Ordering};

/// Filter cutoff from accelerometer (set by main loop, read by ISR)
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
        let base_cutoff = FILTER_CUTOFF.load(Ordering::Relaxed) as f32;
        let env = self.envelope.next_sample();
        let cutoff = base_cutoff + env * 4000.0;
        self.filter.set_cutoff(cutoff);
        let osc_out = self.osc.next_sample();
        let filtered = self.filter.process(osc_out);
        let amp = if env > 1.0 { 1.0 } else { env };
        filtered * amp
    }
}
