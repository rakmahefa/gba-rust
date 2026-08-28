# gba-rust — Roadmap

> Living roadmap. Phase C and Phase D are complete for their deterministic architectural runtime scopes; Phase E is complete.

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
- [x] Implement deterministic serial receive and transfer-completion state.
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
- [x] Phase C branch maintained the repository CI format/check/test/clippy gates before finalization.

### Phase C boundary

Phase C is complete for the project's **deterministic architectural audio/serial scope**. This is not a claim of cycle-perfect analog/audio hardware emulation, external multiplayer transport, or host audio backend fidelity. Those remain outside the runtime contract.

## Phase D — Cartridge and external memory — COMPLETE
- [x] Deterministic SRAM read/write and address mirroring foundation.
- [x] Flash command-state foundation: unlock, byte program, sector erase, chip erase.
- [x] Flash128K bank switching foundation.
- [x] Deterministic EEPROM serial protocol foundation for 512B and 8KiB devices.
- [x] Deterministic WAITCNT decode for SRAM and WS0/WS1/WS2 ROM timings.
- [x] Deterministic eight-halfword Game Pak prefetch buffer model.
- [x] Save-device regression coverage across SRAM, Flash and EEPROM protocol fixtures.
- [x] Phase D implementation keeps cartridge timing/save semantics isolated from CPU/codegen concerns.

### Phase D engineering gate — MET
- [x] SRAM mirroring is deterministic and independently tested.
- [x] Flash programming, sector/chip erase and 128 KiB bank selection are deterministic and regression-tested.
- [x] EEPROM 512B and 8KiB command/data transactions are independently regression-tested.
- [x] WAITCNT timing fields are decoded into explicit ROM/SRAM wait-state parameters.
- [x] Prefetch state is deterministic, bounded to eight halfwords and explicitly invalidatable.
- [x] Repository `cargo test`, `cargo check`, `cargo clippy -D warnings` and rustfmt CI gates remain the phase acceptance criteria.

### Phase D boundary

Phase D is complete for the project's **deterministic cartridge/external-memory architectural baseline**. This does not claim cycle-perfect cartridge bus arbitration, silicon-specific prefetch refill behavior, or every vendor-specific EEPROM/Flash command extension. Those remain compatibility refinements to be driven by real-ROM validation in Phase F.

## Phase E — Generated execution performance — COMPLETE
- [x] E0: deterministic generated-dispatch benchmark baseline.
- [x] E1: static CFG transitions skip redundant runtime CFG-membership probes while retaining alignment validation.
- [x] E2: direct generated-block linking via `GeneratedLinkedBlock` / `run_generated_linked`.
- [x] E3: block chaining/hot-path dispatch reduction through direct successor function pointers.
- [x] E4: runtime boundary minimization for proven static paths without weakening architectural correctness.
- [x] Deterministic benchmark comparison between contract dispatch and direct linked execution.

### Phase E engineering gate — MET
- [x] Static generated transitions avoid redundant CFG membership probes.
- [x] Direct linked successors bypass address/mode redispatch.
- [x] Multi-step hot paths remain inside the linked execution loop.
- [x] Alignment checks remain enforced at static link boundaries.
- [x] Dynamic/exceptional boundaries remain explicit and architecturally validated.
- [x] Benchmark reports identical step counts, zero linked-path CFG probes, and host `ns/step` comparison.
- [x] CI acceptance gates remain green for Phase E changes.

### Phase E boundary

Phase E completes the project's **generated execution dispatch optimization scope**. It does not claim global host-performance optimality or eliminate architectural runtime crossings where timing, memory, IRQ, exception, or dynamic-target semantics require them. Absolute benchmark timings remain host-dependent.

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
