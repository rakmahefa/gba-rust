# gba-rust — Roadmap

> Living roadmap. Phase C is active: deterministic audio/serial runtime contracts are established and APU timing/FIFO integration is progressing toward full PSG, Direct Sound mixing and DMA-trigger fidelity.

## Phase C — Input, audio and serial — IN PROGRESS

### C1. Audio/serial architectural baseline — COMPLETE
- [x] Deterministic APU sample-clock accumulator using the GBA master clock.
- [x] Sound FIFO A/B state with architectural capacity and deterministic byte ordering.
- [x] APU FIFO reset primitives for later timer/FIFO trigger integration.
- [x] Sound MMIO register descriptors with explicit width/access/mask policy.
- [x] SIO control/data register descriptors and a deterministic serial state model.

### C2. APU timing and Direct Sound — IN PROGRESS
- [x] Integrate APU sample-clock advancement into `Runtime::advance_cycles` across scheduler boundaries.
- [x] Connect SOUNDCNT_H MMIO state to the APU runtime model.
- [x] Connect 32-bit FIFO A/B MMIO writes directly to the architectural FIFOs.
- [x] Consume Direct Sound FIFO samples on the selected timer overflow.
- [x] Track deterministic FIFO consumption and underrun counts.
- [x] Add runtime regression fixtures for sample-clock advancement and timer/FIFO selection.
- [x] Implement deterministic PSG frame-sequencer clock at 512 Hz.
- [x] Implement PSG length-counter evolution and channel disable on expiry.
- [x] Implement PSG envelope evolution at the frame-sequencer envelope step.
- [x] Keep sweep as an explicit integration point without inventing register state.
- [x] Add regression fixtures for frame-sequencer boundaries, length expiry and envelope stepping.
- [ ] Implement exact PSG waveform/channel generation for square, wave and noise channels.
- [ ] Implement exact Direct Sound refill/trigger semantics and DMA interaction.
- [ ] Implement SOUNDCNT routing/volume semantics and SOUNDBIAS behavior.
- [ ] Add deterministic audio waveform/mixing regression fixtures.

### C3. Serial/SIO behavior
- [ ] Integrate SIO registers into the runtime MMIO read/write path.
- [ ] Implement normal 8/32-bit serial transfer timing.
- [ ] Implement multiplayer link transfer state and deterministic peer abstraction.
- [ ] Implement UART baseline and serial IRQ routing.
- [ ] Add deterministic SIO regression fixtures.

### C4. DMA audio/special triggers
- [ ] DMA1/DMA2 FIFO destination restrictions.
- [ ] Timer-driven FIFO special-trigger requests.
- [ ] PPU HBlank/VBlank special-trigger arbitration with audio sources.
- [ ] DMA FIFO underrun/priority regression coverage.

### Phase C engineering gate — ACTIVE
- [x] Keep host audio output outside the architectural runtime layer.
- [x] Keep APU clocking on the central runtime scheduler rather than introducing a second clock.
- [x] Keep FIFO state deterministic and independently testable.
- [x] Preserve existing `-D warnings` / Clippy policy.
- [x] C2 timing/FIFO changes maintain workspace format/check/test/clippy CI gates.
- [x] PSG frame-sequencer state evolution is deterministic and covered by unit fixtures.
- [ ] Keep exact PSG waveform generation, Direct Sound refill, SIO transfer timing and DMA special-trigger behavior covered by deterministic runtime fixtures.

### Phase C boundary

Phase C is intentionally being implemented from deterministic device contracts outward. No host audio backend or external link transport belongs in the architectural runtime contract.

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
