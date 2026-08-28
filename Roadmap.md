# gba-rust — Roadmap

> Living roadmap. Phase B now establishes the deterministic PPU rendering/composition baseline; remaining cross-device hardware fidelity is tracked in later phases.

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

## Phase B — Video / PPU fidelity — COMPLETE

**Scope:** deterministic PPU rendering and scanline composition through the runtime HBlank boundary, including display modes, affine BGs, normal/affine OBJ, windows, mosaic baseline and color-effect composition.

### B1. Timing and scanline state

- [x] Nominal scheduler timing: 1004 HDraw + 228 HBlank cycles per scanline, 160 VDraw + 68 VBlank lines per frame.
- [x] Deterministic scanline/HBlank regression coverage.
- [x] VBlank/HBlank/VCOUNT state and IRQ integration through the central runtime timeline.
- [x] PPU rendering is triggered from the HBlank event boundary.

### B2. Display modes and backgrounds

- [x] Register-backed DISPCNT/BG state synchronized at HBlank.
- [x] Mode 0 text BG baseline: BG0/BG1 tile maps, scrolling, 4bpp/8bpp, palette banks and tile flips.
- [x] Mode 1/2 affine BG baseline: signed affine parameters, reference points, map-size handling and 8bpp tile lookup.
- [x] Mode 3 bitmap baseline: 240×160 BGR555.
- [x] Mode 4 bitmap baseline: 8-bit indexed framebuffer, palette and frame select.
- [x] Mode 5 bitmap baseline: 160×128 BGR555 with display centering behavior.
- [x] Layered BG candidate selection with deterministic priority ordering.

### B3. OBJ / OAM

- [x] OBJ renderer isolated from scanline orchestration.
- [x] Normal 4bpp/8bpp OBJ decoding.
- [x] H/V flips, palette transparency and OBJ priority baseline.
- [x] 1D/2D OBJ tile mapping baseline.
- [x] Affine OBJ matrix decoding from the architectural OAM parameter gaps.
- [x] Affine OBJ rendering and double-size geometry baseline.
- [x] OBJ-window path and semi-transparent OBJ classification.
- [x] Integrate OBJ rendering into the PPU scanline/HBlank pipeline.
- [x] Deterministic OBJ ↔ BG ordering through the shared layer compositor.

### B4. Window and blending

- [x] WIN0/WIN1/OBJWIN masking baseline.
- [x] Wrapped horizontal/vertical window intervals.
- [x] BLDCNT layer targeting baseline.
- [x] BLDALPHA alpha blending baseline with bounded EVA/EVB coefficients.
- [x] BLDY brightness increase/decrease baseline.
- [x] Mosaic horizontal composition baseline for BG/OBJ pixels.

### B5. Regression and integration

- [x] Frame-level deterministic regression fixtures for Modes 3 and 4.
- [x] Deterministic sprite/OBJ scanline integration regression coverage.
- [x] Shared layered-compositor regression for equal-priority BG/OBJ ordering.
- [x] Affine BG/OBJ regression fixtures.
- [x] PPU effects unit regression coverage for Window, blending and brightness.
- [x] HBlank integration coverage spanning BG/bitmap + affine + OBJ + effects.

### Phase B engineering gate — MET

- [x] Split display-mode rendering out of `ppu.rs` before adding further PPU complexity.
- [x] Keep affine rendering isolated in `ppu_affine.rs` rather than growing the scanline orchestrator.
- [x] Keep OBJ/OAM decoding isolated in `ppu_sprites.rs`.
- [x] Keep Window/mosaic/color effects isolated in `ppu_effects.rs`.
- [x] Integrate hardware rendering through the PPU scanline boundary rather than maintaining disconnected render paths.
- [x] No compiler IR/codegen expansion was required for the PPU work.
- [x] New PPU features carry deterministic regression coverage.

### Phase B exit criteria — MET

- [x] Display modes 0–5 have deterministic rendering paths or explicit affine dispatch.
- [x] Normal and affine OBJ paths are integrated into the shared compositor.
- [x] Window and color-effect composition is represented in the runtime PPU pipeline.
- [x] Central scheduler timing drives PPU scanline rendering.
- [x] Workspace format/check/test/clippy gates are maintained on the phase branch.

### Phase B boundary / deferred fidelity

Phase B is **complete for its deterministic renderer/compositor scope**, not a claim of cycle-perfect GBA LCD emulation. Native 5-bit rounding minutiae, per-object/per-layer mosaic timing nuances, complete window ordering corner cases and PPU-driven DMA FIFO/special triggers remain compatibility work in later phases and are not hidden behind the Phase B completion label.

## Phase C — Input, audio and serial

- [x] KEYINPUT/KEYCNT architectural baseline (completed in Phase A).
- [ ] APU timing and channels.
- [ ] Serial/SIO behavior.
- [ ] Remaining SIO-related IRQ sources.
- [ ] DMA FIFO/special-trigger integration shared with PPU/APU timing.

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
