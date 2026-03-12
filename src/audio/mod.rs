//! Audio output subsystem

/// Audio sample rate in Hz
pub const SAMPLE_RATE: f32 = 22_050.0;

pub mod dac;
pub mod engine;
pub mod envelope;
pub mod filter;
pub mod oscillator;
