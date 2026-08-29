# Phase F1 — Architectural boot evidence

This document records the automated architectural prerequisites added after the human-validated FireRed checkpoint.

## Covered boundaries

The `gba-cli` integration test `f1_architectural_boot.rs` covers four boundaries:

1. **Cartridge reset contract**
   - System mode at cartridge entry.
   - ARM state at cartridge entry.
   - PC at `0x0800_0000`.
   - System/User stack at `0x0300_7f00`.
   - IRQ stack at `0x0300_7fa0`.
   - Supervisor stack at `0x0300_7fe0`.
   - Central runtime clock starts at zero.

2. **First timer overflow on the central clock**
   - Timer 0 is enabled from a known reload value.
   - One machine cycle advances the timer.
   - The overflow is observed on the same runtime/scheduler clock.
   - Timer IRQ request state becomes pending through the normal interrupt controller.

3. **Architectural IRQ entry from Thumb execution**
   - A pending Timer 0 IRQ is serviced from Thumb state.
   - CPU enters IRQ mode and ARM state.
   - PC moves to the architectural IRQ vector `0x18`.
   - LR preserves the resume address.
   - The banked IRQ stack remains the configured boot stack.
   - SPSR preserves the previous Thumb state.

4. **Generated execution IRQ boundary**
   - A pending timer IRQ is sampled before the next generated block dispatch.
   - Generated execution resumes at the IRQ vector rather than silently continuing the cartridge block stream.
   - No instruction-by-instruction interpreter is introduced by this test.

## Evidence boundary

These tests validate the runtime architectural contract. They do **not** claim that FireRed actually reaches a timer/IRQ event at this exact synthetic configuration.

The real-ROM requirement remains human-observable:

- execute the selected FireRed ROM through the existing static-recompiled path;
- capture the first real timer/interrupt activity encountered beyond the current 4096-block checkpoint;
- compare the resulting architectural boundary against the reference emulator;
- classify any discrepancy as generated-control-flow, compiler/code-generation, runtime, or timing/hardware divergence.

Until that real-ROM evidence is collected, F1 remains open even though its synthetic architectural prerequisites are covered.

## Canonical real-ROM command

```bash
GBA_REAL_ROM="roms/1636 - Pokemon Fire Red (U)(Squirrels).gba" \
cargo test -p gba-cli --test real_rom_execution -- --nocapture
```

The existing deterministic F1 test remains the reference checkpoint; this document only records the additional architectural test coverage.
