//! SAMD51 DAC driver for audio output
//!
//! Configures DAC channel 0 (output on PA2) for direct sample writes.
//! This is the only module that uses unsafe code.

#![allow(unsafe_code)]

use hal::target_device::DAC;
use trellis_m4 as hal;

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
        dac.dacctrl[0].modify(|_, w| w.enable().set_bit().cctrl().cc100k());

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
        // Convert [-1.0, 1.0] -> [0, 4095]
        let clamped = sample.clamp(-1.0, 1.0);
        let value = ((clamped + 1.0) * 2047.5) as u16;

        self.dac.data[0].write(|w| unsafe { w.data().bits(value) });
    }
}
