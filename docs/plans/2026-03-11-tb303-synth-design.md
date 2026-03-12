# NeoBirth TB-303 Acid Synth — Design Document

## Overview

Complete NeoBirth as a TB-303-style acid house synthesizer for the Adafruit NeoTrellis M4 Express (SAMD51). Replace the non-functional PureZen dependency with purpose-built DSP modules. Use the 4x8 button grid as a step sequencer, the accelerometer for real-time filter control, and the NeoPixel grid for visual feedback.

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Synthesis engine | Custom TB-303 emulation (not PureZen) | PureZen v0.0.2 has zero DSP — only message infrastructure |
| Audio output | Direct 12-bit DAC register programming | Only viable analog output; no HAL support exists |
| Sample rate | 22.05kHz, timer interrupt, direct writes | Simple, ~5400 cycles/sample budget at 120MHz is plenty |
| Button grid | 8-step sequencer (bank-switchable to 16) | Matches TB-303 paradigm and 4x8 physical layout |
| Accelerometer | Tilt → filter cutoff, flip → pattern bank | Continuous + discrete control from one sensor |
| LED display | Color = pitch, brightness = accent, highlight = current step | 1:1 mapping with button grid |
| Unsafe handling | Isolated `dac.rs` with `#[allow(unsafe_code)]`, safe API | Preserves `#![deny(unsafe_code)]` everywhere else |

## Architecture

```
src/
├── main.rs              — Peripheral init, main loop (UI polling at ~60Hz)
├── colors.rs            — Color constants + note-to-color gradient function
├── audio/
│   ├── mod.rs           — Audio subsystem init, TC0 interrupt handler (22kHz ISR)
│   ├── dac.rs           — SAMD51 DAC register wrapper [#[allow(unsafe_code)]]
│   ├── oscillator.rs    — Phase-accumulator saw/square oscillator
│   ├── filter.rs        — 2-pole resonant state-variable lowpass filter
│   └── envelope.rs      — Exponential decay envelope (filter depth + amplitude)
├── sequencer/
│   ├── mod.rs           — Playback coordinator, bank switching
│   ├── pattern.rs       — Step storage: note, octave, accent, slide, rest
│   └── clock.rs         — BPM clock via TC1 timer, step advancement
└── ui/
    ├── mod.rs           — UI subsystem init
    ├── controls.rs      — Keypad scanning + accelerometer tilt-to-cutoff mapping
    └── leds.rs          — NeoPixel sequencer visualization
```

## Audio Engine

### Signal Chain

```
Oscillator (saw/square) → Resonant LPF → Decay Envelope → 12-bit DAC
```

All processing at `f32` — the SAMD51 Cortex-M4F has a hardware FPU for single-precision float.

### Oscillator (`audio/oscillator.rs`)

Phase accumulator generating sawtooth or square waveforms. A `Waveform` enum selects the shape. Phase increments per sample based on note frequency. Output normalized to `[-1.0, 1.0]`.

Slide (portamento) implemented as exponential pitch interpolation between consecutive notes when the slide flag is set on a step.

### Filter (`audio/filter.rs`)

2-pole (12dB/oct) state-variable filter with cutoff and resonance parameters. Two state variables (`low`, `band`) updated per sample. Self-oscillates at high resonance — this is desirable for the acid sound.

Cutoff frequency modulated by two sources summed together:
- Decay envelope (per-note, with accent increasing depth)
- Accelerometer tilt (real-time, mapped from X-axis angle)

Resonance is a global parameter (could be fixed or mapped to a UI control later).

### Envelope (`audio/envelope.rs`)

Single-stage exponential decay. On note trigger: resets to 1.0 and decays toward 0.0 at a configurable rate. Applied to both filter cutoff depth and output amplitude.

Accent behavior: when a step has accent enabled, envelope peak increases and decay time shortens, producing the punchy, spiky character of accented 303 notes.

### DAC Driver (`audio/dac.rs`)

The only `#[allow(unsafe_code)]` module. Configures SAMD51 DAC channel 0:
- Enable DAC in MCLK APB D mask
- Configure clock source via GenericClockController
- Set voltage reference
- Expose `fn write_sample(sample: f32)` — converts `[-1.0, 1.0]` to 12-bit unsigned (`0..4095`) and writes to the DATA register

### Timer Interrupt

TC0 configured at 22,050Hz fires `#[interrupt] fn TC0()`. The ISR:
1. Clears interrupt flag
2. Checks for new-note trigger flag (set by sequencer clock)
3. If triggered: resets envelope, updates oscillator frequency
4. Runs: `oscillator.next()` → `filter.process()` → `envelope.apply()`
5. Writes result to DAC

Budget: ~5,400 CPU cycles at 120MHz. The DSP chain uses ~200-300 cycles.

## Sequencer Engine

### Pattern Storage (`sequencer/pattern.rs`)

Each step is packed into a small struct:
- **Note**: 4 bits (0-15, one chromatic octave)
- **Octave offset**: 2 bits (-1, 0, +1)
- **Accent**: 1 bit
- **Slide**: 1 bit
- **Rest**: 1 bit (step is silent)

Two pattern banks (A and B), 8 steps each. Total: 48 bytes in static memory. Stored in `cortex_m::interrupt::Mutex<RefCell<>>` for safe access between main loop and ISR.

### Clock (`sequencer/clock.rs`)

TC1 timer configured for the desired BPM at 16th-note subdivisions. On each tick:
- Advance step index (wrapping 0-7)
- Load next step's parameters
- Set note trigger flag (`AtomicBool`) for the audio ISR to pick up
- Update current step index (`AtomicU8`) for LED display

BPM range: ~60-200. Default: 120 BPM.

### Playback Coordinator (`sequencer/mod.rs`)

Manages which bank is active (A or B, toggled by accelerometer flip). Handles edit-while-playing: the main loop writes to non-current steps or the non-active bank while the clock reads the active bank.

## UI Layer

### Controls (`ui/controls.rs`)

Polled in the main loop at ~60Hz (not interrupt-driven).

**Keypad (4×8 matrix):**
- Row 0: Note selection — press a column to cycle through notes for that step
- Row 1: Accent toggle — press to toggle accent on that step
- Row 2: Slide toggle — press to toggle slide on that step
- Row 3: Utility — pattern clear, waveform select, octave shift, tempo tap

Simple state machine: play mode (view sequence) vs. edit mode (hold column to select step).

**Accelerometer (ADXL343, read at ~30Hz):**
- X-axis tilt angle → filter cutoff frequency (exponential mapping, clamped ~80Hz-8kHz)
- Landscape orientation flip → pattern bank A/B toggle (existing detection logic)

### LED Display (`ui/leds.rs`)

Updates all 32 NeoPixels each main loop iteration:
- **Playhead**: Active step column pulses bright white as sequence advances
- **Note pitch**: Color from a 12-color gradient (warm/red = low, cool/blue = high)
- **Accent**: Accented steps at full brightness, normal steps at ~40%
- **Slide**: Steps with slide blend color toward the next step's color
- **Rest**: Dark/off

`colors.rs` expands to include `fn note_color(note: u8) -> Color` for gradient interpolation.

## Main Loop

```rust
#[entry]
fn main() -> ! {
    // 1. Existing peripheral init (clocks, pins, delay)
    // 2. DAC init (configure registers, enable channel)
    // 3. Audio timer init (TC0 at 22,050Hz, enable interrupt)
    // 4. Sequencer clock init (TC1 at BPM rate)
    // 5. Keypad init
    // 6. Enable interrupts (audio + clock)

    loop {
        // Read accelerometer → update filter cutoff + check bank flip
        // Scan keypad → update pattern data / handle UI actions
        // Update LEDs from current sequencer state
        // delay ~16ms
    }
}
```

## Shared State (main loop ↔ ISR)

| Data | Type | Direction |
|------|------|-----------|
| Filter cutoff | `AtomicU16` | main → audio ISR |
| Current step index | `AtomicU8` | clock ISR → main (for LEDs) |
| Note trigger flag | `AtomicBool` | clock ISR → audio ISR |
| Pattern data | `Mutex<RefCell<[Pattern; 2]>>` | main ↔ clock ISR |
| Active bank | `AtomicU8` | main → clock ISR |

## Dependency Changes

**Remove:**
- `purezen` — not functional, not needed

**Add:**
- `micromath` — no_std float math (sin, cos, sqrt for filter coefficients)
- `cortex-m` — pin version explicitly for interrupt macros (already transitive)

**Keep:**
- `trellis_m4` (with `keypad-unproven` + `adxl343` features)
- `smart-leds`, `ws2812-nop-samd51`
- `panic-halt`

## Implementation Phases

### Phase 1 — DAC "Hello Sound"
Configure DAC registers. Set up TC0 at 22kHz. Output a fixed-frequency sawtooth. Validates the audio output path end-to-end.

### Phase 2 — Oscillator + Filter + Envelope
Build the three DSP modules. Wire into the ISR. Hardcode a repeating note. Target: recognizable squelchy 303 bass tone.

### Phase 3 — Sequencer Engine
Pattern storage, clock timer, step advancement. Hardcode a test pattern. Target: 8-step acid bassline playing automatically.

### Phase 4 — Keypad + Accelerometer UI
Keypad scanning and step editing. Accelerometer-to-cutoff mapping. Bank switching. Target: program patterns by pressing buttons, sweep filter by tilting.

### Phase 5 — LED Feedback
Replace static colors with sequencer visualization. Playhead, note colors, accent brightness. Target: full interactive synth with visual feedback.

## Risks

- **DAC pin routing**: Must verify which physical pin/pad connects to an accessible output on the NeoTrellis M4 PCB. The DAC peripheral exists but board routing needs confirmation.
- **Timer interrupt priority**: Audio ISR (TC0) must have higher priority than clock ISR (TC1) to avoid audio glitches. Configure NVIC priorities explicitly.
- **Flash size**: SAMD51G19A has 512KB flash. This firmware should fit in <64KB comfortably.
