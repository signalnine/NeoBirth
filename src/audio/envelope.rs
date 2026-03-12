//! Exponential decay envelope generator

/// Exponential decay envelope
pub struct Envelope {
    /// Current envelope value
    pub value: f32,
    /// Current decay coefficient per sample
    pub decay: f32,
    /// Peak level for normal trigger
    pub peak: f32,
    /// Peak level for accented trigger
    pub accent_peak: f32,
    /// Decay coefficient for normal notes
    pub normal_decay: f32,
    /// Decay coefficient for accented notes
    pub accent_decay: f32,
}

/// Compute the per-sample decay coefficient for a given time in milliseconds
fn decay_coefficient(ms: f32) -> f32 {
    1.0 - (4.6 / (ms * super::SAMPLE_RATE / 1000.0))
}

impl Envelope {
    /// Create a new envelope with default settings
    ///
    /// Normal decay: 200ms, accent decay: 100ms
    pub fn new() -> Self {
        let normal_decay = decay_coefficient(200.0);
        let accent_decay = decay_coefficient(100.0);

        Self {
            value: 0.0,
            decay: normal_decay,
            peak: 1.0,
            accent_peak: 1.5,
            normal_decay,
            accent_decay,
        }
    }

    /// Set the normal decay time in milliseconds
    pub fn set_decay_ms(&mut self, ms: f32) {
        self.normal_decay = decay_coefficient(ms);
    }

    /// Trigger the envelope
    ///
    /// If `accent` is true, uses accent peak and accent decay; otherwise uses
    /// normal peak and normal decay.
    pub fn trigger(&mut self, accent: bool) {
        if accent {
            self.value = self.accent_peak;
            self.decay = self.accent_decay;
        } else {
            self.value = self.peak;
            self.decay = self.normal_decay;
        }
    }

    /// Generate the next envelope sample and advance the state
    pub fn next_sample(&mut self) -> f32 {
        let out = self.value;
        self.value *= self.decay;

        // Kill denormals
        if self.value < 0.0001 {
            self.value = 0.0;
        }

        out
    }

    /// Get the current envelope value without advancing
    pub fn value(&self) -> f32 {
        self.value
    }
}
