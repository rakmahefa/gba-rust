# BIOS static execution reference

`gba_bios.bin` is the first architectural end-to-end execution fixture for the static recompiler.

The reference path is:

1. Map the BIOS image at `0x00000000` in ARM state.
2. Recover the reachable CFG from the BIOS entry point.
3. Lower to typed and semantic IR.
4. Generate deterministic Rust blocks.
5. Load the same BIOS bytes into `gba-runtime` at the BIOS address range.
6. Execute the generated blocks through the runtime generated-execution contract.
7. Stop deterministically on `Halt`, `Return`, or the configured step limit.

This is intentionally a runtime/recompiler architectural fixture, not yet a claim of full GBA hardware compatibility. Memory-mapped I/O, DMA, timers, IRQ scheduling, PPU, APU and other hardware behavior remain separate workstreams.
