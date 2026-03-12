# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

NeoBirth is a bare-metal Rust embedded firmware for the **Adafruit NeoTrellis M4 Express** (SAMD51 ARM Cortex-M4F). It's an acid house music synthesizer powered by the PureZen engine (Pure Data for embedded Rust). The board has a 4x8 NeoPixel LED grid, ADXL343 accelerometer, and capacitive keypad.

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

The entire firmware is two files in `src/`:
- **main.rs** — Entry point, peripheral initialization (clocks, pins, delay, NeoPixels, accelerometer), main loop (reads orientation, writes LEDs)
- **colors.rs** — RGB color constants (WHITE, YELLOW, ORANGE, RED)

## Key Constraints

- Target is `thumbv7em-none-eabihf` (configured in `.cargo/config`)
- The crate uses strict lints: `#![deny(warnings, unsafe_code, missing_docs, unused_import_braces, unused_qualifications)]`
- Release profile optimizes for size (`opt-level = "s"`)
- `trellis_m4` dependency is patched to use the git version from `atsamd-rs/atsamd`
- Rust edition 2018

## Required Toolchain

- Rust stable with target: `rustup target add thumbv7em-none-eabihf`
- GNU ARM Embedded Toolchain (`arm-none-eabi-objcopy`, `arm-none-eabi-gdb`)
- BOSSA flash utility for programming the device
