//! Phase-accumulator oscillator with saw and square waveforms

#![allow(dead_code)]

/// Waveform type
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Waveform {
    /// Sawtooth waveform
    Saw,
    /// Square waveform
    Square,
}

/// Phase-accumulator oscillator
pub struct Oscillator {
    /// Current phase in [0.0, 1.0)
    pub phase: f32,
    /// Current phase increment per sample
    pub phase_inc: f32,
    /// Target phase increment for slide
    pub target_phase_inc: f32,
    /// Slide rate (exponential interpolation factor per sample)
    pub slide_rate: f32,
    /// Current waveform
    pub waveform: Waveform,
}

impl Oscillator {
    /// Create a new oscillator with default settings
    pub fn new() -> Self {
        Self {
            phase: 0.0,
            phase_inc: 0.0,
            target_phase_inc: 0.0,
            slide_rate: 1.0,
            waveform: Waveform::Saw,
        }
    }

    /// Set the oscillator frequency immediately (no slide)
    pub fn set_frequency(&mut self, freq: f32) {
        let inc = freq / super::SAMPLE_RATE;
        self.phase_inc = inc;
        self.target_phase_inc = inc;
    }

    /// Set the target frequency to slide toward
    pub fn slide_to_frequency(&mut self, freq: f32) {
        self.target_phase_inc = freq / super::SAMPLE_RATE;
    }

    /// Set the slide rate (0.0 = instant, closer to 1.0 = slower slide)
    pub fn set_slide_rate(&mut self, rate: f32) {
        self.slide_rate = rate;
    }

    /// Set the waveform type
    pub fn set_waveform(&mut self, wf: Waveform) {
        self.waveform = wf;
    }

    /// Generate the next sample in [-1.0, 1.0]
    pub fn next_sample(&mut self) -> f32 {
        // Exponential interpolation of phase_inc toward target
        self.phase_inc += (self.target_phase_inc - self.phase_inc) * (1.0 - self.slide_rate);

        // Advance phase
        self.phase += self.phase_inc;

        // Wrap phase to [0.0, 1.0)
        while self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        while self.phase < 0.0 {
            self.phase += 1.0;
        }

        // Generate waveform sample
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
