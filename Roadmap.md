# gba-rust — Roadmap

> Living roadmap. The runtime-correctness baseline is now complete; remaining hardware-specific fidelity work is tracked in the later phases where it belongs.

## Current direction

`gba-rust` is a static GBA recompiler with a dedicated ARM7TDMI/runtime layer. The compiler pipeline is established enough to prioritize hardware fidelity and sustained real-ROM execution over adding new compiler abstractions.

## Phase A — Runtime correctness — COMPLETE

**Goal:** establish a deterministic architectural runtime boundary covering CPU state, MMIO, timing, interrupts, DMA/timers, bus-backed memory and generated-block execution.

### A1. MMIO contract

- [x] Central register descriptors with width/access policy/writable masks.
- [x] Core interrupt, display, keypad, WAITCNT, IME, POSTFLG and HALTCNT contracts.
- [x] DMA and timer register descriptors.
- [x] Byte/halfword access coverage for architectural registers.
- [x] KEYCNT keypad selection, OR/AND mode and keypad IRQ request semantics.
- [x] Architectural read/write masks and reserved-bit handling covered by runtime tests.

### A2. Timers

- [x] Four timers with reload/counter/control state.
- [x] Prescalers and cycle accumulation.
- [x] Cascade overflow propagation.
- [x] Timer MMIO integration.
- [x] Timer IRQ generation through runtime scheduling.
- [x] Enable/disable transition handling.
- [x] Reload-based cadence for multiple overflows in one large cycle jump.
- [x] Regression coverage for chained overflow boundaries.

### A3. DMA

- [x] Four DMA channels and priority arbitration.
- [x] Immediate/VBlank/HBlank/Special trigger model.
- [x] 16/32-bit transfer modes.
- [x] Address increment/decrement/fixed/reload behavior.
- [x] Zero-count architectural expansion without mutating CNT_L.
- [x] Waitstate-aware transfer timing model.
- [x] DMA IRQ integration.
- [x] Repeat and destination-reload regression coverage.
- [x] Deterministic transfer lifecycle and register-visible post-transfer state.

**Deferred hardware-fidelity work:** DMA1/DMA2 FIFO/special-trigger restrictions are retained for the video/audio integration phases because their correctness depends on PPU/APU event sources.

### A4. Scheduler and timing

- [x] Monotonic cycle clock.
- [x] Deterministic event ordering by cycle and insertion sequence.
- [x] PPU/DMA/timer integration points.
- [x] Timer → IRQ and DMA → IRQ timing regression coverage.
- [x] Deterministic same-cycle scheduling baseline.
- [x] Hardware event chains have explicit integration points for HBlank/VBlank and completion/IRQ propagation.

### A5. Runtime integration

- [x] Architectural CPU/runtime boundary.
- [x] BIOS exception/IRQ path.
- [x] Bus-backed memory regions.
- [x] Deterministic generated-block execution boundary.
- [x] Keypad-driven IRQ wake-up path.
- [x] HALT/STOP runtime boundary and wake-up integration points.
- [x] Interrupt source routing and acknowledgement baseline.
- [x] End-to-end runtime correctness fixtures independent of commercial ROMs.

### Phase A exit criteria — MET

- [x] `cargo fmt --all -- --check` passes.
- [x] `cargo test --workspace --all-targets` passes.
- [x] `cargo check --workspace --all-targets` passes.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [x] Runtime tests cover MMIO access policy, timers, DMA, IRQ routing and scheduler ordering.
- [x] Real-ROM execution remains deterministic after the Phase A changes.

### Phase A boundary

Phase A establishes **runtime correctness infrastructure**, not complete GBA hardware emulation. Hardware-specific fidelity that requires concrete PPU/APU/SIO/cartridge behavior is intentionally moved into the corresponding later phases rather than keeping Phase A permanently open.

## Phase B — Video / PPU fidelity

**Current focus:** keep PPU rendering modular and deterministic. `ppu.rs` owns scanline orchestration/register synchronization; `ppu_modes.rs` owns bitmap/text display-mode rendering; `ppu_affine.rs` owns affine BG modes 1/2; `ppu_sprites.rs` owns OBJ/OAM rendering.

### B1. Timing and scanline state

- [ ] PPU timing and scanline state.
- [ ] VBlank/HBlank/VCOUNT IRQ behavior.

### B2. Display modes and backgrounds

- [ ] DISPCNT/BG/affine/window/blending semantics.
- [ ] Modes 0–5.
- [x] Text BG baseline: Mode 0 BG0/BG1 tile maps, scrolling, 4bpp/8bpp and tile flips.
- [x] Bitmap baseline: Modes 3, 4 and 5.
- [x] Affine BG baseline for Modes 1/2, including signed affine parameters, reference points, map-size handling and 8bpp tile lookup.

### B3. OBJ / OAM

- [ ] Sprite/OAM behavior.
- [x] OBJ renderer isolated from scanline orchestration.
- [x] Normal 4bpp/8bpp OBJ decoding, flips, palette transparency and priority baseline.

### B4. Window and blending

- [ ] WIN0/WIN1/OBJWIN masking semantics.
- [ ] BLDCNT layer targeting.
- [ ] BLDALPHA alpha blending.
- [ ] BLDY brightness increase/decrease.

### B5. Regression and DMA integration

- [x] Frame-level deterministic regression fixtures for Modes 3 and 4.
- [ ] Frame-level regression fixtures for all implemented display modes.
- [ ] DMA1/DMA2 FIFO/special-trigger integration with PPU timing.

### Phase B engineering gate

- [x] Split display-mode rendering out of `ppu.rs` before adding further PPU complexity.
- [x] Keep affine rendering isolated in `ppu_affine.rs` rather than growing the scanline orchestrator.
- [ ] No compiler IR/codegen expansion for PPU work unless a concrete compatibility requirement demands it.
- [ ] Every new display feature gets deterministic scanline or frame-level regression coverage.

## Phase C — Input, audio and serial

- [x] KEYINPUT/KEYCNT architectural baseline (completed in Phase A).
- [ ] APU timing and channels.
- [ ] Serial/SIO behavior.
- [ ] Remaining SIO-related IRQ sources.
- [ ] DMA FIFO integration with APU timing.

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

## Engineering rules

- Do not expand the compiler IR/codegen architecture unless a concrete runtime or compatibility requirement demands it.
- Do not treat "real ROM compiles and exits deterministically" as equivalent to "game is playable".
- Do not require commercial ROMs in CI.
- Preserve deterministic scheduling and explicit architectural boundaries as later hardware components are added.
