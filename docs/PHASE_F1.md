# Phase F1 — Deterministic real-ROM boot

Phase F1 extends the Phase F0 real-ROM harness from pipeline validation to a reproducible boot checkpoint.

## Scope

The selected ROM remains user-provided through `GBA_REAL_ROM` and is never embedded in the repository.

The automated F1 scenario:

1. validates the cartridge header;
2. performs static CFG analysis;
3. generates Rust from the recovered CFG;
4. starts the generated program against `gba-runtime`;
5. executes a fixed 4096-generated-block boot window;
6. records PC, ARM/Thumb state, SP, cycle count and termination reason;
7. repeats the same boot window;
8. compares the architectural checkpoint and generated-block trace byte-for-byte.

The production CPU path remains generated Rust blocks. No instruction-by-instruction interpreter is introduced by F1.

## Automated acceptance evidence

F1 automation proves:

- the cartridge can be loaded into the same runtime contract used by F0;
- the generated execution path progresses beyond the cartridge entry point;
- the boot checkpoint contains architectural state required for comparison;
- repeated runs produce the same checkpoint;
- repeated runs produce the same generated-block trace.

For the selected FireRed ROM, the local test completed two deterministic boot runs with:

```text
size=16777216
 title=POKEMON FIRE
 game_code=BPRE
 entry_target=0x08000204
 steps=4096
 pc=0x081dc81c
 thumb=true
 sp=0x03007e08
 cycles=22132
 exit=StepLimitExceeded
```

A deterministic checkpoint is evidence of reproducibility, not by itself evidence of the intended game-visible milestone.

## Human validation — completed

The checkpoint was independently inspected with the mGBA debugger using the same FireRed ROM and a Thumb breakpoint at `0x081DC81C`.

mGBA stopped with:

```text
PC/current instruction = 0x081DC81E
instruction address     = 0x081DC81C
Thumb                    = true
SP                       = 0x03007E08
Cycle                    = 20257
instruction              = BE00  bkpt
```

This establishes that `0x081DC81C` is a genuine execution point in FireRed's real code path, with the same Thumb state and SP as the `gba-rust` checkpoint. The differing cycle counters are intentionally not treated as equivalent timing measurements; rigorous timing comparison belongs to F3.

Human validation result:

**F1 checkpoint humain : VALIDÉ.**

The checkpoint is therefore accepted as a reference-correlated F1 execution point rather than an arbitrary plateau.

## Remaining F1 engineering work

Human checkpoint validation does not close the entire F1 phase. The following architectural work remains:

- validate the complete reset/initialization path used by the selected ROM;
- validate ARM/Thumb transitions across the broader boot path;
- validate supervisor/IRQ exception entry used by the boot sequence;
- validate early memory initialization;
- validate the first timer/interrupt activity encountered by the ROM;
- distinguish compiler/code-generation divergence from runtime/hardware divergence beyond the current checkpoint.

## Canonical local command

```bash
GBA_REAL_ROM="roms/1636 - Pokemon Fire Red (U)(Squirrels).gba" \
cargo test -p gba-cli --test real_rom_execution -- --nocapture
```

The F1 test is `real_rom_boot_checkpoint_is_deterministic` and uses a fixed 4096-generated-block window.

## Exit condition for F1

F1 can be marked fully complete only after the remaining reset/initialization, exception/IRQ, early-memory and first timer/interrupt evidence is covered. The validated checkpoint is a prerequisite milestone, not a substitute for those architectural checks.

F2 then targets real exception, BIOS and IRQ execution as an explicit validation phase.
