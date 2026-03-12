# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

NeoBirth is a bare-metal Rust embedded firmware for the **Adafruit NeoTrellis M4 Express** (SAMD51 ARM Cortex-M4F). It's a TB-303-style acid house synthesizer with a sawtooth/square oscillator, resonant lowpass filter, decay envelope, and 8-step sequencer. The board has a 4x8 NeoPixel LED grid, ADXL343 accelerometer, and capacitive keypad.

This is a `#![no_std]` / `#![no_main]` project — no heap, no OS.

## Build Commands

```bash
# Build (CARGO_INCREMENTAL=0 required for reproducible embedded builds)
CARGO_INCREMENTAL=0 cargo build --release

# Create flashable binary
arm-none-eabi-objcopy -O binary target/thumbv7em-none-eabihf/release/neobirth neobirth.bin

# Flash to device (double-tap reset to enter bootloader, device mounts as USB drive)
# Linux:
bossac -p /dev/ttyACM0 -e -w -v -R --offset=0x4000 neobirth.bin
# macOS:
bossac -p /dev/cu.usbmodem* -e -w -v -R --offset=0x4000 neobirth.bin

# Lint and format (CI checks these)
cargo fmt -- --check
cargo clippy
```

There are no tests — this is firmware with no test harness.

## Architecture

```
src/
├── main.rs              — Peripheral init, timer/interrupt setup, main loop (UI at ~60Hz)
├── colors.rs            — RGB color constants + note-to-color gradient
├── audio/
│   ├── mod.rs           — SAMPLE_RATE constant (22,050 Hz), module declarations
│   ├── dac.rs           — SAMD51 DAC register wrapper (only unsafe_code module)
│   ├── engine.rs        — SynthVoice: glue between oscillator/filter/envelope
│   ├── oscillator.rs    — Phase-accumulator saw/square oscillator with slide
│   ├── filter.rs        — Chamberlin 2-pole resonant state-variable lowpass
│   └── envelope.rs      — Exponential decay envelope with accent support
├── sequencer/
│   ├── mod.rs           — Sequencer state, atomic step/trigger/bank flags
│   └── pattern.rs       — Step/Pattern/PatternBank storage, MIDI freq table
└── ui/
    ├── mod.rs           — Module declarations
    ├── controls.rs      — Keypad 4x8 matrix scanning with edge detection
    └── leds.rs          — NeoPixel sequencer visualization
```

**Interrupt architecture:** TC0 fires at 22kHz running the DSP chain (oscillator→filter→envelope→DAC). TC1 fires at ~8Hz advancing the sequencer. Main loop polls keypad + accelerometer at ~60Hz. Shared state uses `Mutex<RefCell<Option<T>>>` and atomics.

**GCLK routing:** The HAL lacks clock methods for DAC and TC0/TC1, so GCLK PCHCTRL registers are configured via raw pointer access (indices 9 for TC0_TC1, 37 for DAC).

## Key Constraints

- Target is `thumbv7em-none-eabihf` (configured in `.cargo/config`)
- Strict lints: `#![deny(warnings, missing_docs, unused_import_braces, unused_qualifications)]`
- `unsafe_code` is allowed only in `audio/dac.rs` and interrupt handlers in `main.rs`
- Release profile optimizes for size (`opt-level = "s"`)
- `trellis_m4` dependency is patched to use git version from `atsamd-rs/atsamd`
- `cortex-m` requires `const-fn` feature for static Mutex initialization
- DAC output is on PA2 (analog pin A0), NOT the PA6/PA7 audio pins
- Rust edition 2018

## Required Toolchain

- Rust stable with target: `rustup target add thumbv7em-none-eabihf`
- GNU ARM Embedded Toolchain (`arm-none-eabi-objcopy`, `arm-none-eabi-gdb`)
- BOSSA flash utility for programming the device
