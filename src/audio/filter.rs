//! 2-pole resonant state-variable lowpass filter (Chamberlin)

#![allow(dead_code)]

/// Chamberlin state-variable lowpass filter
pub struct Filter {
    /// Low-pass output state
    pub low: f32,
    /// Band-pass output state
    pub band: f32,
    /// Cutoff coefficient (derived from frequency)
    pub cutoff: f32,
    /// Resonance amount (0.0 to 0.99)
    pub resonance: f32,
}

impl Filter {
    /// Create a new filter with default settings
    pub fn new() -> Self {
        Self {
            low: 0.0,
            band: 0.0,
            cutoff: 0.45,
            resonance: 0.0,
        }
    }

    /// Set the cutoff frequency in Hz
    ///
    /// Converts to a coefficient and clamps to [0.001, 0.45]
    pub fn set_cutoff(&mut self, freq: f32) {
        let f = 2.0 * core::f32::consts::PI * freq / super::SAMPLE_RATE;
        // Clamp coefficient to safe range
        if f < 0.001 {
            self.cutoff = 0.001;
        } else if f > 0.45 {
            self.cutoff = 0.45;
        } else {
            self.cutoff = f;
        }
    }

    /// Set the resonance amount (clamped to [0.0, 0.99])
    pub fn set_resonance(&mut self, res: f32) {
        if res < 0.0 {
            self.resonance = 0.0;
        } else if res > 0.99 {
            self.resonance = 0.99;
        } else {
            self.resonance = res;
        }
    }

    /// Process one input sample through the filter, returning the lowpass output
    ///
    /// Uses two iterations per sample for improved stability at high frequencies.
    pub fn process(&mut self, input: f32) -> f32 {
        let q = 1.0 - self.resonance;

        // Two iterations for stability
        for _ in 0..2 {
            self.low += self.cutoff * self.band;
            let high = input - self.low - q * self.band;
            self.band += self.cutoff * high;
        }

        self.low
    }

    /// Reset filter state to zero
    pub fn reset(&mut self) {
        self.low = 0.0;
        self.band = 0.0;
    }
}
