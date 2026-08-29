# Phase F1 — Deterministic real-ROM boot

Phase F1 extends the Phase F0 real-ROM harness from pipeline validation to a reproducible boot checkpoint and reference-correlated early boot evidence.

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

### Cartridge reset and handoff

The selected FireRed ROM was restarted in mGBA and observed from the BIOS reset boundary.

At reset:

```text
PC    = 0x00000004
SP    = 0x03007F00
CPSR  = 0x0000001F
Cycle = 0
```

The first cartridge execution was then observed at:

```text
PC/current instruction = 0x08000000
PC next                = 0x08000004
SP                     = 0x03007F00
CPSR                   = 0x2000001F
Thumb                  = false
Cycle                  = 28
instruction            = EA00007F  b 0x08000204
```

Continuing reached the declared cartridge entry target:

```text
PC/current instruction = 0x08000204
PC next                = 0x08000208
SP                     = 0x03007F00
LR                     = 0x08000000
CPSR                   = 0x2000001F
Thumb                  = false
Cycle                  = 48
instruction            = E3A00012  mov r0, #18
```

This confirms that the FireRed header `entry_target=0x08000204` is not merely metadata: the ROM's actual early boot code branches from `0x08000000` to `0x08000204`.

### First ARM → Thumb transition

The early ARM disassembly shows:

```text
08000228:  E59F1014  ldr r1, =0x080003A5
0800022C:  E1A0E00F  mov lr, pc
08000230:  E12FFF11  bx r1
```

Because `r1 = 0x080003A5` is odd, the BX selects Thumb state and the architectural target is `0x080003A4`.

mGBA confirmed the transition by stopping at:

```text
PC/current instruction = 0x080003A4
PC next                = 0x080003A6
SP                     = 0x03007E40
LR                     = 0x08000234
CPSR                   = 0x0000003F
Thumb                  = true
Cycle                  = 175
instruction            = B5F0  stmdb sp!, {r4-r7,lr}
```

Therefore the first observed real-ROM ARM → Thumb transition is:

```text
0x08000230  BX 0x080003A5
        ↓
0x080003A4  Thumb
```

### F1 checkpoint reference correlation

The checkpoint was independently inspected with the mGBA debugger using the same FireRed ROM and a breakpoint at `0x081DC81C`.

The current observed mGBA state was:

```text
PC/current instruction = 0x081DC81E
instruction address     = 0x081DC81C
Thumb                    = true
SP                       = 0x03007E08
Cycle                    = 20257
instruction              = C008  stmia r0!, {r3}
```

This establishes that `0x081DC81C` is a genuine execution point in FireRed's real code path, with the same Thumb state and SP as the `gba-rust` checkpoint. The differing cycle counters are intentionally not treated as equivalent timing measurements; rigorous timing comparison belongs to F3.

**Human validation result so far:**

**F1 checkpoint humain : VALIDÉ.**

The reset boundary, cartridge entry, declared entry target, first observed ARM → Thumb transition and reference-correlated checkpoint are now documented as real FireRed observations.

## Remaining F1 engineering work

The following items remain open because they have not yet been demonstrated from the real ROM itself:

- validate the complete reset/initialization path used by the selected ROM;
- validate supervisor/IRQ exception entry used by the boot sequence;
- validate early memory initialization against expected boot behavior;
- validate the first timer/interrupt activity encountered by the ROM;
- distinguish compiler/code-generation divergence from runtime/hardware divergence beyond the current checkpoint.

The first ARM → Thumb transition is now observed and documented, but this does not yet prove all subsequent state transitions across the complete boot path.

## Evidence boundary

The synthetic architectural tests in `crates/gba-cli/tests/f1_architectural_boot.rs` validate the runtime contract for reset, timer overflow, IRQ entry and generated-dispatch IRQ sampling. These tests do not claim that FireRed reaches the same timer/IRQ configuration.

The real-ROM manual evidence above remains the authority for FireRed-specific boot observations.

## Canonical local command

```bash
GBA_REAL_ROM="roms/1636 - Pokemon Fire Red (U)(Squirrels).gba" \
cargo test -p gba-cli --test real_rom_execution -- --nocapture
```

The F1 test is `real_rom_boot_checkpoint_is_deterministic` and uses a fixed 4096-generated-block window.

## Exit condition for F1

F1 can be marked fully complete only after the remaining reset/initialization, exception/IRQ, early-memory and first timer/interrupt evidence is covered. The validated checkpoint is a prerequisite milestone, not a substitute for those architectural checks.

F2 then targets real exception, BIOS and IRQ execution as an explicit validation phase.
