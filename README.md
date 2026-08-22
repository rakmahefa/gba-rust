# gba-rust

**gba-rust** is a Rust-first **static Game Boy Advance recompilation project**.

The project analyzes executable code from a GBA ROM, recovers reachable control flow and functions, lowers instructions into typed and semantic intermediate representations, performs conservative effect-aware optimization, and generates Rust code backed by a separate GBA runtime.

> **Recompiler-first, not interpreter-first.** The architectural goal is statically generated native Rust/LLVM code plus explicit runtime services, rather than a conventional instruction-by-instruction interpreter.

Inspired by the engineering direction of [`arcanite24/gb-recompiled`](https://github.com/arcanite24/gb-recompiled), but implemented as an independent Rust-first GBA architecture.

## Current status

`gba-rust` has moved well beyond the initial decoder/CFG prototype into a multi-stage static recompilation pipeline with an explicit ARM7TDMI execution contract.

The current `main` includes:

- ARM and original GBA Thumb (ARMv4T) decoding;
- reachable CFG recovery with ARM/Thumb-aware addresses;
- dedicated CFG discovery, edge construction, abstract-state analysis, partitioning and hardening;
- basic-block partitioning and function discovery;
- typed IR and semantic IR with structural validation;
- explicit register, flag, memory and control-flow effects;
- conservative effect-aware optimization;
- deterministic Rust code generation;
- an explicit ARM7TDMI architectural runtime model;
- deterministic generated-block execution with halt, exception and step-limit behavior;
- architectural regression fixtures and differential generated-execution validation;
- BIOS SWI/exception execution foundations;
- architectural exception entry/return and reentrant IRQ execution;
- BIOS exception-graph analysis and real BIOS exception-vector execution tests;
- GBA bus/memory classification foundations;
- timing-scheduler foundations for PPU, DMA and IRQ boundaries;
- SRAM, Flash and EEPROM save-device foundations;
- Phase 7 real-ROM execution and hardware-boundary validation.

The project is **not yet a complete playable GBA implementation**. The compiler pipeline is substantially established, so the next major effort is hardware fidelity and larger real-ROM coverage: complete MMIO behavior, DMA execution, timers, PPU rendering, keypad/input, APU, cartridge timing/protocol behavior and a robust generated-block dispatch/linking path.

## Architecture

```text
                         GBA ROM (.gba)
                              |
                              v
                 +---------------------------+
                 |      gba-recompiler       |
                 |                           |
                 | ARM / Thumb decoder       |
                 | CFG recovery              |
                 | Function discovery        |
                 | Typed IR                  |
                 | Semantic IR               |
                 | Conservative optimizer    |
                 | Rust code generation      |
                 +-------------+-------------+
                               |
                               v
                        Generated Rust
                               |
                               v
                 +---------------------------+
                 |        gba-runtime         |
                 |                           |
                 | ARM7TDMI state           |
                 | Exception / BIOS model   |
                 | Bus / memory map         |
                 | Cartridge / saves        |
                 | Timing / hardware        |
                 +-------------+-------------+
                               |
                               v
                        Native Rust / LLVM
```

The layers deliberately have different responsibilities:

- **`gba-recompiler`** analyzes ROM code and produces CFGs, IR, semantic representations and generated Rust.
- **Generated Rust** represents statically recovered program logic and emits explicit execution-boundary transitions.
- **`gba-runtime`** provides CPU state, exception handling, memory, cartridge and hardware-facing services consumed by generated code.
- **`gba-cli`** is the development harness for ROM analysis, generated-source emission and runtime bootstrapping.
- **`gba-core`** contains lower-level CPU/bus/cartridge foundations and is currently outside the root workspace.
- **`gba-egui`** contains an `egui`/`eframe` frontend prototype and is currently outside the root workspace.

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the architectural contract.

## Workspace

The root Cargo workspace currently contains:

```text
crates/
├── gba-recompiler/
│   └── ARM/Thumb decoding, CFG, functions, IR, optimization and codegen
├── gba-runtime/
│   └── ARM7TDMI state, exceptions, memory, cartridge/save and runtime services
└── gba-cli/
    └── ROM analysis and generated-source development harness
```

Additional prototype crates remain outside the root workspace:

```text
crates/
├── gba-core/
│   └── CPU, bus and cartridge foundations
└── gba-egui/
    └── egui/eframe analysis/debug frontend
```

The workspace uses **Rust 2021**. Release builds use thin LTO, one codegen unit and `panic = "abort"`.

## Static recompiler pipeline

### 1. Decode

Instructions retain their address, size, execution mode and decoded operation. ARM and original GBA Thumb decoding are organized into focused modules for instruction families, memory operations, classification and shared semantic helpers.

GBA CPUs use **ARM state and the original ARMv4T Thumb instruction set**. This project is therefore **not a Thumb-2 recompiler**.

### 2. Discover reachable code

Analysis starts from an explicit ROM entry address and execution mode. Statically known successors are followed while preserving ARM/Thumb mode in the CFG key.

Unknown instructions and unresolved dynamic targets constrain static discovery instead of inventing unsafe successors.

### 3. Build and harden the CFG

CFG leaders come from the entry point and control-flow targets. Blocks retain their execution mode and stop at control-flow boundaries.

```text
cfg/
├── abstract_state.rs
├── discovery.rs
├── edges.rs
├── hardening.rs
├── model.rs
├── partition.rs
└── mod.rs
```

Validation protects invariants such as instruction ownership, address continuity, block identity and successor references. Conservative abstract register information can resolve some indirect targets, including ARM/Thumb exchange cases.

### 4. Recover functions

Function analysis records:

- function entry blocks;
- direct call sites;
- function-to-function successors;
- return sites;
- return continuations;
- unresolved indirect call/branch cases.

### 5. Lower to typed IR

The typed IR keeps architectural operations explicit. Representative operations include `Mov`, arithmetic/logical operations, `Cmp`, `Load`, `Store`, branches, exchange operations, `Nop` and `Unknown`.

IR instructions expose architectural effects such as register reads/writes, memory access width/kind, flags and control flow.

### 6. Build semantic IR

The semantic layer makes control flow and side effects explicit. Semantic blocks can terminate with fallthrough, conditional branch, direct/indirect call, return, indirect branch, exception or unknown behavior.

Structural validation protects block ownership, instruction identity, successor validity, call continuations and consistency between recovered functions and semantic blocks.

### 7. Optimize conservatively

The optimizer currently favors transformations justified by the modeled semantics, including identity-move elimination, `Add 0` / `Sub 0` normalization, local constant propagation and local constant folding.

Optimization is effect-aware: register, flag, memory and control-flow effects constrain transformations so correctness remains the priority.

### 8. Generate Rust

`gba-recompiler` emits deterministic Rust representing recovered program structure and basic blocks. Generated code goes through the runtime execution contract for registers, memory, flags, condition evaluation, PC/Thumb state, control flow and architectural exceptions.

Generated Rust is currently a **development and execution artifact**, not a packaged standalone game executable.

### 9. Execute generated blocks deterministically

The runtime exposes explicit generated-block exits and boundaries for normal control flow, exceptions, halts and step limits.

```text
ROM
 |
 v
static analysis
 |
 +-- CFG / functions
 +-- semantic IR
 +-- generated Rust
          |
          v
    generated block
          |
          v
    runtime boundary
       /       \
      /         \
 normal       exception
  transition     |
      |          v
      |      exception entry
      |          |
      +----<-----+
```

The compiler/runtime boundary validates target alignment and linked-CFG membership rather than silently dispatching arbitrary addresses.

## ARM7TDMI execution model

`gba-runtime` contains an explicit ARM7TDMI architectural state model rather than a collection of generic integer helpers.

Current foundations include:

- user/system, FIQ, IRQ, Supervisor, Abort and Undefined modes;
- banked SP/LR registers and FIQ banked `r8-r12`;
- SPSR handling for exception modes;
- CPSR `N/Z/C/V`, interrupt masks, mode bits and Thumb state;
- condition-code evaluation;
- ARM/Thumb exchange through BX;
- architectural PC and link-address rules;
- ARM shift edge cases and ROR behavior;
- add/subtract carry and borrow semantics;
- ARM unaligned word rotation behavior;
- architectural exception entry and restoration.

Architectural primitives remain independently testable where practical, keeping runtime correctness separate from recompiler logic.

## BIOS and exception execution

BIOS-triggered exceptions are modeled as an architectural execution boundary instead of an unrelated helper call.

### SWI boundary

For generated BIOS SWIs:

1. the runtime enters Supervisor mode;
2. exception state is captured in the architectural CPU state;
3. the modeled BIOS service executes against runtime state;
4. returning SWIs restore the caller's CPSR and banked registers;
5. non-returning services such as HALT/STOP retain the expected privileged state.

Generated BIOS services now include regression coverage for memory-transfer SWIs such as `CpuSet` and `CpuFastSet`.

### IRQ boundary

Generated execution observes pending enabled IRQs at a **block boundary**. The dispatcher establishes the architectural PC for that boundary and then reuses the runtime exception-entry contract.

This preserves the interrupted resume point, CPSR state and banked IRQ registers without mutating CPU mode in the middle of a generated instruction.

### Exception returns

ARM instructions that write `PC` with the `S` bit set are treated as architectural exception returns. The generated path evaluates the return target, asks the runtime to restore the active SPSR/banked state, and only then performs the CFG transition.

Ordinary `BX LR`/function returns remain separate from architectural exception restoration.

### Reentrant exceptions

Nested exception entry reuses the same boundary model. An IRQ taken while privileged can capture the current mode state, use IRQ banked registers and later restore the interrupted context through the architectural exception-return primitive.

### BIOS exception graph

The recompiler exposes BIOS exception-graph analysis so exception vectors, handlers and return sites can participate in static analysis. Real BIOS exception vectors are exercised through generated code in regression fixtures.

## Bus and memory contract

The runtime uses a single address classifier before device-specific semantics. This establishes a canonical physical address mapping that generated CPU access, DMA and future timing-aware devices can share.

Current address-space foundations include:

- BIOS `0x00000000-0x00003FFF`;
- EWRAM `0x02000000-0x02FFFFFF`, mirrored over 256 KiB;
- IWRAM `0x03000000-0x03FFFFFF`, mirrored over 32 KiB;
- MMIO `0x04000000-0x040003FF` as an explicit device boundary;
- palette RAM `0x05000000-0x05FFFFFF`, mirrored over 1 KiB;
- VRAM `0x06000000-0x06FFFFFF` with GBA-specific mirroring;
- OAM `0x07000000-0x07FFFFFF`, mirrored over 1 KiB;
- Game Pak ROM windows `0x08000000-0x0DFFFFFF`;
- SRAM/Flash `0x0E000000-0x0FFFFFFF`.

Device semantics remain separate from address classification. Timing, waitstates, DMA arbitration and complete MMIO behavior are layered on top of this contract.

## Timing and scheduler

`gba-runtime::TimingScheduler` is the single monotonic machine clock for time-driven hardware.

Generated CPU execution advances the scheduler through runtime cycle advancement. Events are deterministic and ordered by cycle, insertion sequence and event kind.

Current scheduler foundations cover:

- PPU HBlank boundaries;
- scanline and VBlank transitions;
- DMA completion boundaries;
- explicit IRQ sampling boundaries;
- continuous timer advancement between hardware boundaries.

The architecture deliberately separates:

1. time progression;
2. hardware side effects;
3. architectural CPU transitions.

This keeps asynchronous hardware deterministic while preserving block-boundary exception semantics.

## Cartridge and saves

Battery-backed storage belongs to the cartridge model. It is **not an emulator savestate**.

Supported save-device foundations include SRAM, Flash and EEPROM variants.

Persistence uses `<game>.sav`, with `<game>.sav.bak` retained when possible. Writes are dirty-tracked and flushed atomically. Save-type detection and matching-size save reuse are part of the cartridge layer.

## CLI and real-ROM validation

The development CLI is used for ROM analysis, generated-source emission and runtime bootstrapping.

A real-ROM regression path is available through `GBA_REAL_ROM` and validates the complete development boundary:

```text
real .gba ROM
    |
    v
cartridge mapping
    |
    v
static CFG analysis
    |
    v
generated Rust
    |
    v
runtime cartridge preflight
    |
    v
compiled temporary generated runner
    |
    v
deterministic generated execution
```

The validation checks cartridge reads, entry-state assumptions, generated-code compilation, architectural PC alignment and the final generated-execution exit.

Example:

```bash
GBA_REAL_ROM=/path/to/game.gba \
  cargo test -p gba-cli --test real_rom_execution -- --nocapture
```

The real-ROM test is intentionally opt-in so public CI does not depend on distributing copyrighted commercial ROM images.

## Development ROMs

Development/test ROM inputs may exist under `roms/`, including FireRed and Emerald images used for local analysis.

Use ROMs only when you have the legal right to use them. The project does not distribute commercial game ROMs.

## Frontend prototype

`gba-egui` is a separate `egui`/`eframe` frontend prototype backed by `gba-core`. It remains outside the root workspace while its APIs are still evolving.

Its intended role is to provide a native analysis/debugging surface without coupling UI concerns to the recompiler core.

## Validation strategy

Validation is intentionally cross-stage because modules can be individually correct while disagreeing at their boundaries.

Current layers include:

1. ARM7TDMI architectural primitive tests;
2. decoder and CFG regression fixtures;
3. semantic IR structural/effect validation;
4. semantic-to-codegen integration tests;
5. deterministic generated-execution fixtures;
6. exception and IRQ regression fixtures;
7. differential tests for selected instruction semantics;
8. BIOS exception-vector execution fixtures;
9. real-ROM CFG/runtime boundary validation when a ROM is supplied.

The preferred development progression is:

```text
unit semantics
    ↓
synthetic ROM fixtures
    ↓
generated execution
    ↓
BIOS/exception execution
    ↓
real-ROM CFG validation
    ↓
real-ROM execution
    ↓
stable game boot
```

## CI

GitHub Actions currently validates the workspace with:

```bash
cargo fmt
cargo test --workspace --all-targets
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

Real-ROM execution remains opt-in because CI cannot assume access to commercial ROM images.

## Roadmap

The project is transitioning from **compiler architecture completion** toward **hardware fidelity and real-ROM execution coverage**.

### Phase 7 — Real-ROM execution and hardware boundary validation

**Status: complete on `main`.**

The phase established:

- real-ROM cartridge mapping at `0x08000000`;
- real-ROM CFG analysis;
- generated Rust emission from a real ROM;
- runtime cartridge preflight;
- temporary generated-runner compilation and execution;
- architectural PC-alignment checks;
- generated-execution exit validation.

### Phase 8 — Hardware fidelity and real-ROM coverage

**Status: next major phase.**

The priority is no longer adding another large compiler abstraction. The dominant risk is now the generated-code/runtime/hardware boundary.

#### 8.1 Memory and MMIO correctness

- complete the GBA memory-map contract;
- implement register-level MMIO semantics incrementally;
- add read/write side-effect tests;
- validate wait-state-sensitive regions where required;
- preserve a single canonical bus decoder for CPU and DMA.

#### 8.2 DMA

- implement channel configuration and transfer semantics;
- model source/destination address control;
- model repeat and timing modes;
- connect transfer completion to scheduler events and IRQ requests;
- add deterministic DMA-vs-CPU regression fixtures.

#### 8.3 Timers and interrupt sources

- complete timer register behavior;
- validate cascading timers;
- align overflow timing with the central scheduler;
- expand IRQ-source behavior;
- validate IRQ sampling and exception entry against real generated execution.

#### 8.4 PPU execution contract

- implement display modes 0-5 incrementally;
- complete HBlank/VBlank/VCOUNT behavior;
- add OAM/sprite and window semantics;
- build deterministic scanline/frame regression fixtures;
- keep rendering correctness separate from CPU exception transitions.

#### 8.5 Keypad/input and remaining MMIO

- implement `KEYINPUT` behavior;
- implement relevant interrupt behavior;
- complete remaining high-value MMIO devices required by real ROM startup and main loops.

#### 8.6 Cartridge protocol and timing

- complete SRAM/Flash command protocols;
- complete EEPROM serial protocol behavior;
- model relevant timing restrictions;
- expand save detection and persistence fixtures.

#### 8.7 Generated dispatch/linking hardening

- formalize block-key and alignment invariants;
- expand linked-block validation;
- reduce fallback exits for statically resolvable targets;
- introduce safe block chaining where correctness is proven;
- add stress fixtures for ARM/Thumb transitions, loops and indirect branches.

#### 8.8 Real-ROM execution coverage

Use legal local ROM fixtures to expand validation from entry-point analysis to stable execution regions:

```text
ROM entry
   ↓
authorized boot code
   ↓
BIOS interactions
   ↓
memory initialization
   ↓
IRQ/timer setup
   ↓
VBlank/main loop
   ↓
stable generated execution
```

The first concrete milestone is **stable real-ROM execution**, not yet full gameplay rendering/audio.

### Phase 9 — Playable GBA runtime

After Phase 8 establishes reliable hardware boundaries:

- complete PPU rendering;
- complete APU/audio output;
- keypad/input integration;
- complete cartridge timing;
- broader instruction coverage;
- robust generated-block linking;
- deterministic save/load behavior;
- frontend integration.

The first major acceptance target is a deterministic boot of a substantial real GBA title, followed by stable frame execution and input processing.

### Phase 10 — Native performance

Only after architectural fidelity is sufficiently strong:

- block chaining;
- constant propagation across safe boundaries;
- memory specialization;
- hot-path inlining;
- cache-aware generated dispatch;
- profiling-guided optimization;
- deterministic performance benchmarks.

Performance work must not bypass the architectural runtime contract or weaken exception/hardware correctness.

## Design principles

`gba-rust` intentionally follows a few strict rules:

1. **Recompiler-first** — generated native Rust/LLVM is the execution center.
2. **Architectural correctness before optimization** — unsafe transformations are deferred.
3. **Explicit effects** — registers, flags, memory and control flow are modeled instead of inferred ad hoc.
4. **Exceptions at boundaries** — asynchronous and architectural transitions do not mutate generated instructions mid-flight.
5. **One runtime contract** — generated code, CPU, BIOS and hardware share explicit interfaces.
6. **Determinism** — CFGs, generated sources, scheduling and regression fixtures should be reproducible.
7. **Conservative unknowns** — unresolved dynamic behavior is represented as unknown instead of guessed.
8. **Layered hardware** — address classification, device semantics, timing and architectural CPU transitions remain distinct.

## License

The workspace metadata currently declares the project under the **MIT** license. ROM images are separate copyrighted works and are not covered by the project license.
