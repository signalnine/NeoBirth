//! Audio synthesis engine
//!
//! TB-303 style signal chain: oscillator -> resonant lowpass filter -> envelope -> output

pub mod envelope;
pub mod filter;
pub mod oscillator;

/// Audio sample rate in Hz (shared by all DSP modules)
pub const SAMPLE_RATE: f32 = 22_050.0;
