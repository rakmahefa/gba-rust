# gba-rust — Roadmap

> Living roadmap. Phase C is complete for the deterministic architectural audio/serial runtime scope; Phase D is next.

## Current direction

`gba-rust` is a static GBA recompiler with a dedicated ARM7TDMI/runtime layer. The compiler pipeline is established enough to prioritize hardware fidelity and sustained real-ROM execution over adding new compiler abstractions.

## Phase C — Input, audio and serial — COMPLETE

### C1. Audio/serial architectural baseline — COMPLETE
- [x] Deterministic APU sample-clock accumulator using the GBA master clock.
- [x] Sound FIFO A/B state with architectural capacity and deterministic byte ordering.
- [x] APU FIFO reset primitives.
- [x] Sound MMIO register descriptors with explicit width/access/mask policy.
- [x] SIO control/data register descriptors and deterministic serial state.

### C2. APU / PSG / Direct Sound — COMPLETE
- [x] Integrate APU sample-clock advancement into `Runtime::advance_cycles` across scheduler boundaries.
- [x] Integrate the 512 Hz PSG frame sequencer on the central runtime clock.
- [x] PSG length-counter evolution and automatic channel disable on expiry.
- [x] PSG envelope evolution and deterministic timer state.
- [x] Deterministic sweep evolution for the Square 1 channel.
- [x] Deterministic square, wave and noise sample generation.
- [x] Deterministic PSG + Direct Sound sample mixing path.
- [x] Connect SOUNDCNT_L/H/X and SOUNDBIAS runtime state to the APU model.
- [x] Connect 32-bit FIFO A/B MMIO writes directly to the architectural FIFOs.
- [x] Consume Direct Sound FIFO samples on the selected timer overflow.
- [x] Deterministic FIFO refill threshold requests at the low-water mark.
- [x] Deterministic FIFO underrun accounting.
- [x] Runtime regression coverage for sample clock, frame sequencer, PSG evolution, FIFO selection and mixing.

### C3. Serial / SIO — COMPLETE
- [x] Integrate SIO registers into runtime MMIO read/write paths.
- [x] Preserve architectural SIOCNT mode/IRQ bits and RCNT policy.
- [x] Implement deterministic Normal 8-bit transfer timing.
- [x] Implement deterministic Normal 32-bit transfer timing.
- [x] Implement deterministic multiplayer local-peer state abstraction.
- [x] Implement deterministic UART baseline.
- [x] Implement serial receive state and transfer completion state.
- [x] Route serial transfer completion to the GBA serial IRQ source.
- [x] Add deterministic SIO/runtime regression fixtures.

### C4. DMA audio / special triggers — COMPLETE
- [x] Restrict Direct Sound FIFO special triggers to DMA1/DMA2.
- [x] Restrict audio special-trigger destinations to FIFO A/B architectural addresses.
- [x] Generate FIFO refill requests from timer-driven Direct Sound consumption.
- [x] Feed audio refill requests into the central DMA arbitration path.
- [x] Preserve deterministic DMA channel priority when audio and video timing requests coincide.
- [x] Keep PPU HBlank/VBlank DMA arbitration on the same scheduler clock as audio.
- [x] Add DMA FIFO destination/priority regression coverage.

### Phase C engineering gate — MET
- [x] Host audio output remains outside the architectural runtime layer.
- [x] No secondary device clock was introduced; APU and SIO advance from the central scheduler.
- [x] FIFO state is deterministic and independently testable.
- [x] SIO transfer timing and serial IRQ delivery are deterministic.
- [x] DMA audio special triggers remain explicit and restricted to the architectural FIFO destinations.
- [x] Existing `-D warnings` / Clippy policy is preserved.
- [x] Phase C carries runtime unit/integration coverage for audio, serial and DMA interaction.
- [x] Phase C branch maintained the repository CI format/check/test/clippy gates.

### Phase C boundary

Phase C is complete for the project's **deterministic architectural audio/serial scope**. This is not a claim of cycle-perfect analog/audio hardware emulation, external multiplayer transport, or host audio backend fidelity. Those remain outside the runtime contract.

## Phase D — Cartridge and external memory — NEXT
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