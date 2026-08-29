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

A deterministic checkpoint is evidence of reproducibility, not evidence that the observed state is the intended game boot milestone.

## Human validation boundary

Human validation is still required before declaring the boot milestone complete. The user must confirm that the observed state corresponds to the expected boot behavior of the selected ROM and distinguish a genuine milestone from an execution plateau.

In particular, F1 must not be marked complete solely because the test process exits successfully or because the checkpoint is deterministic.

## Canonical local command

```bash
GBA_REAL_ROM="roms/1636 - Pokemon Fire Red (U)(Squirrels).gba" \
cargo test -p gba-cli --test real_rom_execution -- --nocapture
```

The F1 test is `real_rom_boot_checkpoint_is_deterministic` and uses a fixed 4096-generated-block window.

## Next boundary

Once the checkpoint is confirmed as a genuine boot milestone, F1 can move to the remaining reset/initialization and first timer/IRQ evidence. F2 then targets real exception, BIOS and IRQ execution as an explicit validation phase.
