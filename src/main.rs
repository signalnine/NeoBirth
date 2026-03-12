//! TB-303 style acid house synthesizer for the [Adafruit NeoTrellis M4].
//!
//! Features a sawtooth oscillator, resonant lowpass filter, and
//! exponential decay envelope driven by an 8-step sequencer. Audio
//! output at 22,050 Hz via the on-board DAC; sequencer clock via
//! hardware timer interrupt at ~8 Hz (120 BPM sixteenth notes).
//!
//! [Adafruit NeoTrellis M4]: https://learn.adafruit.com/adafruit-neotrellis-m4

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
mod ui;

#[allow(unused_imports)]
use panic_halt;
use trellis_m4 as hal;
use ws2812_nop_samd51 as ws2812;

use core::cell::RefCell;
use cortex_m::interrupt::Mutex;
use hal::adxl343::accelerometer::Accelerometer;
use hal::prelude::*;
use hal::target_device::interrupt;
use hal::{clock::GenericClockController, delay::Delay, entry, CorePeripherals, Peripherals};
use smart_leds::{Color, SmartLedsWrite};

use audio::dac::Dac;
use audio::engine::SynthVoice;
use sequencer::Sequencer;

/// Total number of LEDs on the NeoTrellis M4
const NUM_LEDS: usize = 32;

/// GCLK PCHCTRL index for TC0/TC1 peripheral clock
const PCHCTRL_TC0_TC1: usize = 9;

/// GCLK PCHCTRL index for DAC peripheral clock
const PCHCTRL_DAC: usize = 37;

/// Synth voice shared between main and ISR
static SYNTH: Mutex<RefCell<Option<SynthVoice>>> = Mutex::new(RefCell::new(None));

/// DAC driver shared between main and ISR
static DAC_DRIVER: Mutex<RefCell<Option<Dac>>> = Mutex::new(RefCell::new(None));

/// Sequencer shared between main and ISR
static SEQ: Mutex<RefCell<Option<Sequencer>>> = Mutex::new(RefCell::new(None));

/// Main entrypoint
#[entry]
fn main() -> ! {
    let mut peripherals = Peripherals::take().unwrap();
    let mut core_peripherals = CorePeripherals::take().unwrap();

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

    // --- Accelerometer ---
    let mut adxl343 = pins
        .accel
        .open(
            &mut clocks,
            peripherals.SERCOM2,
            &mut peripherals.MCLK,
            &mut pins.port,
        )
        .unwrap();

    // --- Keypad ---
    let keypad = hal::Keypad::new(pins.keypad, &mut pins.port);
    let mut keypad_state = ui::controls::KeypadState::new();

    // --- DAC setup ---
    // Enable DAC APB clock
    peripherals
        .MCLK
        .apbdmask
        .modify(|_, w| w.dac_().set_bit());

    // Route GCLK0 (120 MHz) to DAC peripheral via PCHCTRL
    // GCLK has been consumed by GenericClockController, access via raw pointer
    unsafe {
        let gclk = &*hal::target_device::GCLK::ptr();
        // CHEN = bit 6, GEN = bits 0..3 (generator 0 = 0x00)
        gclk.pchctrl[PCHCTRL_DAC].write(|w| {
            w.gen().bits(0); // GCLK0
            w.chen().set_bit()
        });
    }

    // Configure DAC pin (PA2 = analog A0) for DAC function (function B)
    let _dac_pin = pins.analog.a0.into_function_b(&mut pins.port);

    let dac = Dac::new(peripherals.DAC);

    // --- Synth and Sequencer ---
    let synth = SynthVoice::new();
    let mut seq = Sequencer::new();
    seq.patterns.load_demo();

    cortex_m::interrupt::free(|cs| {
        SYNTH.borrow(cs).replace(Some(synth));
        DAC_DRIVER.borrow(cs).replace(Some(dac));
        SEQ.borrow(cs).replace(Some(seq));
    });

    // --- Timer setup ---
    // Enable TC0/TC1 peripheral clock via GCLK0
    unsafe {
        let gclk = &*hal::target_device::GCLK::ptr();
        gclk.pchctrl[PCHCTRL_TC0_TC1].write(|w| {
            w.gen().bits(0); // GCLK0
            w.chen().set_bit()
        });
    }

    // Enable TC0 APB clock
    peripherals
        .MCLK
        .apbamask
        .modify(|_, w| w.tc0_().set_bit());

    // Configure TC0 for 22,050 Hz audio sample rate
    // 120 MHz / 16 / 340 = ~22,058 Hz
    {
        let tc = peripherals.TC0.count16();
        // Disable TC0
        tc.ctrla.modify(|_, w| w.enable().clear_bit());
        while tc.syncbusy.read().enable().bit_is_set() {}

        // Set mode to COUNT16, prescaler DIV16
        tc.ctrla.write(|w| w.mode().count16().prescaler().div16());
        while tc.syncbusy.read().enable().bit_is_set() {}

        // Set waveform generation to MFRQ (match frequency)
        tc.wave.write(|w| w.wavegen().mfrq());

        // Set compare value for ~22,050 Hz
        tc.cc[0].write(|w| unsafe { w.cc().bits(340) });

        // Enable MC0 interrupt
        tc.intenset.write(|w| w.mc0().set_bit());

        // Enable TC0
        tc.ctrla.modify(|_, w| w.enable().set_bit());
        while tc.syncbusy.read().enable().bit_is_set() {}
    }

    // Enable TC1 APB clock
    peripherals
        .MCLK
        .apbamask
        .modify(|_, w| w.tc1_().set_bit());

    // Configure TC1 for ~8 Hz sequencer clock (120 BPM 16th notes)
    // 120 MHz / 1024 / 14648 = ~8.0 Hz
    {
        let tc = peripherals.TC1.count16();
        // Disable TC1
        tc.ctrla.modify(|_, w| w.enable().clear_bit());
        while tc.syncbusy.read().enable().bit_is_set() {}

        // Set mode to COUNT16, prescaler DIV1024
        tc.ctrla
            .write(|w| w.mode().count16().prescaler().div1024());
        while tc.syncbusy.read().enable().bit_is_set() {}

        // Set waveform generation to MFRQ
        tc.wave.write(|w| w.wavegen().mfrq());

        // Set compare value for ~8 Hz
        tc.cc[0].write(|w| unsafe { w.cc().bits(14648) });

        // Enable MC0 interrupt
        tc.intenset.write(|w| w.mc0().set_bit());

        // Enable TC1
        tc.ctrla.modify(|_, w| w.enable().set_bit());
        while tc.syncbusy.read().enable().bit_is_set() {}
    }

    // Set NVIC priorities and enable interrupts
    unsafe {
        core_peripherals
            .NVIC
            .set_priority(hal::target_device::Interrupt::TC0, 0);
        core_peripherals
            .NVIC
            .set_priority(hal::target_device::Interrupt::TC1, 2);
        core_peripherals.NVIC.enable(hal::target_device::Interrupt::TC0);
        core_peripherals.NVIC.enable(hal::target_device::Interrupt::TC1);
    }

    // --- Main loop ---
    let mut pixels = [Color::default(); NUM_LEDS];

    loop {
        // Read raw accelerometer for orientation + filter cutoff
        if let Ok(accel) = adxl343.acceleration() {
            let x = f32::from(accel.x);

            // Orientation detection (threshold ~0.5g in ±2G mode)
            let x_abs = if x < 0.0 { -x } else { x };
            if x_abs > 128.0 {
                if x >= 0.0 {
                    sequencer::ACTIVE_BANK
                        .store(0, core::sync::atomic::Ordering::Release);
                } else {
                    sequencer::ACTIVE_BANK
                        .store(1, core::sync::atomic::Ordering::Release);
                }
            }

            // Map X-axis tilt to filter cutoff (80-8000 Hz)
            let normalized = (x + 256.0) / 512.0;
            let clamped = if normalized < 0.0 {
                0.0f32
            } else if normalized > 1.0 {
                1.0f32
            } else {
                normalized
            };
            let cutoff = 80.0 + clamped * 7920.0;
            audio::engine::FILTER_CUTOFF
                .store(cutoff as u16, core::sync::atomic::Ordering::Relaxed);
        }

        // Scan keypad for pattern editing
        let keypad_inputs = keypad.decompose();
        let mut pressed = [[false; 8]; 4];
        for (row_idx, row) in keypad_inputs.iter().enumerate() {
            for (col_idx, button) in row.iter().enumerate() {
                pressed[row_idx][col_idx] = button.is_low();
            }
        }

        if let Some(action) = keypad_state.scan(&pressed) {
            let step_idx = match &action {
                ui::controls::KeyAction::CycleNote(s)
                | ui::controls::KeyAction::ToggleAccent(s)
                | ui::controls::KeyAction::ToggleSlide(s)
                | ui::controls::KeyAction::ClearStep(s) => *s as usize,
            };
            cortex_m::interrupt::free(|cs| {
                if let Some(seq) = SEQ.borrow(cs).borrow_mut().as_mut() {
                    let bank =
                        sequencer::ACTIVE_BANK.load(core::sync::atomic::Ordering::Relaxed)
                            as usize;
                    ui::controls::apply_action(
                        &mut seq.patterns.banks[bank].steps[step_idx],
                        &action,
                    );
                }
            });
        }

        // Update LED display from sequencer state
        let step_idx =
            sequencer::CURRENT_STEP.load(core::sync::atomic::Ordering::Relaxed);
        let bank =
            sequencer::ACTIVE_BANK.load(core::sync::atomic::Ordering::Relaxed) as usize;
        cortex_m::interrupt::free(|cs| {
            if let Some(seq) = SEQ.borrow(cs).borrow().as_ref() {
                ui::leds::render(&mut pixels, &seq.patterns.banks[bank], step_idx);
            }
        });

        neopixel.write(pixels.iter().cloned()).unwrap();

        delay.delay_ms(16u8);
    }
}

/// Audio sample rate interrupt (22,050 Hz)
#[interrupt]
fn TC0() {
    // Clear the MC0 interrupt flag
    let tc = unsafe { &*hal::target_device::TC0::ptr() };
    tc.count16().intflag.write(|w| w.mc0().set_bit());

    cortex_m::interrupt::free(|cs| {
        if let (Some(synth), Some(dac)) = (
            SYNTH.borrow(cs).borrow_mut().as_mut(),
            DAC_DRIVER.borrow(cs).borrow_mut().as_mut(),
        ) {
            // Check for new note trigger from sequencer
            if sequencer::NOTE_TRIGGER.swap(false, core::sync::atomic::Ordering::Acquire) {
                if let Some(seq) = SEQ.borrow(cs).borrow().as_ref() {
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

/// Sequencer clock interrupt (~8 Hz for 120 BPM)
#[interrupt]
fn TC1() {
    // Clear the MC0 interrupt flag
    let tc = unsafe { &*hal::target_device::TC1::ptr() };
    tc.count16().intflag.write(|w| w.mc0().set_bit());

    cortex_m::interrupt::free(|cs| {
        if let Some(seq) = SEQ.borrow(cs).borrow_mut().as_mut() {
            seq.advance();
        }
    });
}
