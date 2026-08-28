# gba-rust — Provisional Roadmap

> Working roadmap. This document is intentionally provisional and will be updated as hardware fidelity and real-ROM execution expose new requirements.

## Current direction

`gba-rust` is a static GBA recompiler with a dedicated ARM7TDMI/runtime layer. The compiler pipeline is already established enough to prioritize runtime correctness and real-ROM fidelity over adding new compiler abstractions.

## Phase A — Runtime correctness

**Goal:** make the runtime's architectural state, MMIO, timing, interrupts and bus/device interactions deterministic and GBA-correct enough to support sustained real-ROM execution.

### A1. MMIO contract

- [x] Central register descriptors with width/access policy/writable masks.
- [x] Core interrupt, display, keypad, WAITCNT, IME, POSTFLG and HALTCNT contracts.
- [x] DMA and timer register descriptors.
- [x] Byte/halfword access coverage for architectural registers.
- [x] KEYCNT keypad selection, OR/AND mode and keypad IRQ request semantics.
- [ ] Complete remaining IRQ-source register semantics, especially SIO.
- [ ] Audit all write-only/read-only and reserved-bit behavior against the GBA register map.

### A2. Timers

- [x] Four timers with reload/counter/control state.
- [x] Prescalers and cycle accumulation.
- [x] Cascade overflow propagation.
- [x] Timer MMIO integration.
- [x] Timer IRQ generation through runtime scheduling.
- [x] Enable/disable transition handling.
- [x] Reload-based cadence for multiple overflows in one large cycle jump.
- [x] Regression coverage for chained overflow boundaries.
- [ ] Validate all edge cases against hardware timing/reference behavior.

### A3. DMA

- [x] Four DMA channels and priority arbitration.
- [x] Immediate/VBlank/HBlank/Special trigger model.
- [x] 16/32-bit transfer modes.
- [x] Address increment/decrement/fixed/reload behavior.
- [x] Zero-count architectural expansion without mutating CNT_L.
- [x] Waitstate-aware transfer timing model.
- [x] DMA IRQ integration.
- [x] Repeat and destination-reload regression coverage.
- [ ] Validate FIFO/special timing semantics for DMA1/DMA2.
- [ ] Audit special-trigger restrictions and register-visible post-transfer state.

### A4. Scheduler and timing

- [x] Monotonic cycle clock.
- [x] Deterministic event ordering by cycle and insertion sequence.
- [x] PPU/DMA/timer integration points.
- [x] Timer → IRQ and DMA → IRQ timing regression coverage.
- [ ] Establish explicit architectural priority rules for same-cycle hardware events where insertion order is not sufficient.
- [ ] Add HBlank/VBlank → DMA → completion → IRQ chain fixtures.

### A5. Runtime integration

- [x] Architectural CPU/runtime boundary.
- [x] BIOS exception/IRQ path.
- [x] Bus-backed memory regions.
- [x] Deterministic generated-block execution boundary.
- [x] Keypad-driven IRQ wake-up path.
- [ ] Harden HALT/STOP wake-up semantics.
- [ ] Complete interrupt source routing and acknowledgement semantics.
- [ ] Add end-to-end runtime correctness fixtures independent of real commercial ROMs.

### Phase A exit criteria

- `cargo fmt --all -- --check` passes.
- `cargo test --workspace --all-targets` passes.
- `cargo check --workspace --all-targets` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- Runtime tests cover MMIO access policy, timers, DMA, IRQ routing and scheduler ordering.
- Real-ROM execution remains deterministic after each Phase A increment.

## Phase B — Video / PPU fidelity

- [ ] PPU timing and scanline state.
- [ ] DISPCNT/BG/affine/window/blending semantics.
- [ ] Modes 0–5.
- [ ] Sprite/OAM behavior.
- [ ] VBlank/HBlank/VCOUNT IRQ behavior.
- [ ] Frame-level regression fixtures.

## Phase C — Input, audio and serial

- [ ] KEYINPUT/KEYCNT semantics.
- [ ] APU timing and channels.
- [ ] Serial/SIO behavior.
- [ ] Associated IRQ sources.

## Phase D — Cartridge and external memory

- [ ] SRAM/Flash/EEPROM behavior.
- [ ] Cartridge waitstates and prefetch fidelity.
- [ ] Save-device protocol regression tests.

## Phase E — Generated execution performance

- [ ] Direct generated-block linking.
- [ ] Block chaining/hot-path dispatch reduction.
- [ ] Runtime boundary minimization without weakening architectural correctness.
- [ ] Deterministic performance benchmarks.

## Phase F — Real game execution

- [ ] Boot a real ROM through BIOS/runtime initialization.
- [ ] Reach a deterministic title screen.
- [ ] Reach interactive execution with input.
- [ ] Validate sustained execution across multiple frames.
- [ ] Establish a reproducible real-ROM compatibility matrix.

## Non-goals for the current phase

- Do not expand the compiler IR/codegen architecture unless a concrete runtime correctness requirement demands it.
- Do not treat "real ROM compiles and exits deterministically" as equivalent to "game is playable".
- Do not require commercial ROMs in CI.
