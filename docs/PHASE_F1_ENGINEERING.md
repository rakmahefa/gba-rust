# Phase F1 — Architectural boot evidence

This document records the automated architectural prerequisites and the real FireRed observations added after the human-validated deterministic checkpoint.

## Covered boundaries

The `gba-cli` integration test `f1_architectural_boot.rs` covers four synthetic runtime boundaries:

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

## Real FireRed manual evidence

The same `POKEMON FIRE` / `BPRE` ROM was inspected in mGBA.

### Reset and cartridge handoff

At reset mGBA reported:

```text
PC    = 0x00000004
SP    = 0x03007F00
CPSR  = 0x0000001F
Cycle = 0
```

The first observed cartridge instruction was:

```text
08000000: EA00007F  b 0x08000204
```

with:

```text
SP    = 0x03007F00
CPSR  = 0x2000001F
Thumb = false
Cycle = 28
```

Continuing reached the ROM header's declared entry target:

```text
08000204: E3A00012  mov r0, #18
```

with `SP=0x03007F00`, ARM state and `Cycle=48`.

### First ARM → Thumb transition

The early boot sequence contains:

```text
08000228: E59F1014  ldr r1, =0x080003A5
0800022C: E1A0E00F  mov lr, pc
08000230: E12FFF11  bx r1
```

The odd target `0x080003A5` selects Thumb state, and mGBA confirmed execution at:

```text
080003A4: B5F0  stmdb sp!, {r4-r7,lr}
```

with:

```text
SP    = 0x03007E40
LR    = 0x08000234
CPSR  = 0x0000003F
Thumb = true
Cycle = 175
```

This is the first real FireRed ARM → Thumb transition captured for F1.

### Reference checkpoint

The deterministic generated boot reaches:

```text
PC    = 0x081DC81C
Thumb = true
SP    = 0x03007E08
cycles = 22132
steps = 4096
exit = StepLimitExceeded
```

mGBA reaches the same instruction address with:

```text
instruction address = 0x081DC81C
current PC           = 0x081DC81E
Thumb                = true
SP                   = 0x03007E08
Cycle                = 20257
instruction          = C008  stmia r0!, {r3}
```

The matching PC location, Thumb state and SP establish reference correlation. Cycle counts are not treated as timing-equivalence evidence; that belongs to F3.

## Evidence boundary

The synthetic tests validate the runtime architectural contract. The manual observations establish real-ROM behavior for FireRed at reset, cartridge handoff, first ARM → Thumb transition and the existing 4096-block checkpoint.

These observations do **not** yet demonstrate:

- the complete reset/initialization path;
- the first real timer programmed and overflowing on the FireRed path;
- the first real hardware IRQ encountered by FireRed;
- supervisor/IRQ exception return behavior on the real ROM path;
- the separation of compiler/code-generation divergence from runtime/hardware divergence beyond the current checkpoint.

F1 therefore remains open.

## Canonical real-ROM command

```bash
GBA_REAL_ROM="roms/1636 - Pokemon Fire Red (U)(Squirrels).gba" \
cargo test -p gba-cli --test real_rom_execution -- --nocapture
```
