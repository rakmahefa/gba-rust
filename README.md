# gba-rust

**gba-rust** is a Rust-first **static Game Boy Advance recompiler**. Its goal is to recover executable GBA code from a ROM, lower it into an intermediate representation, generate Rust source, and execute that generated program against a native Rust GBA runtime.

The project is deliberately **not centered on instruction-by-instruction interpretation**. The long-term execution model is:

```text
GBA ROM
  │
  ▼
ARM / Thumb decoding
  │
  ▼
Reachable CFG recovery
  │
  ▼
Typed GBA IR
  │
  ▼
Rust code generation
  │
  ▼
Native Rust / LLVM
  │
  ▼
GBA runtime
```

> Inspired by the engineering direction of [`arcanite24/gb-recompiled`](https://github.com/arcanite24/gb-recompiled), but implemented as an independent Rust-first GBA architecture.

## Current status

The project has moved beyond the initial architecture/prototype stage and now has a working **static-analysis → CFG → IR → Rust-codegen pipeline** for the currently supported instruction subset.

Implemented today:

- ARM and Thumb instruction decoding infrastructure;
- reachable-code discovery from a ROM entry point;
- ARM/Thumb-aware basic-block and control-flow graph recovery;
- conditional and unconditional branch handling;
- block partitioning that preserves sequential instruction tails;
- typed IR lowering for the currently decoded operations;
- deterministic Rust source generation from the recovered CFG;
- a native Rust runtime boundary used by generated code;
- GBA cartridge/save modeling for SRAM, Flash and EEPROM variants;
- dirty tracking and atomic-ish save-file replacement with `.sav.bak` backup;
- CLI support for ROM analysis and generated-source emission;
- workspace tests and GitHub Actions validation.

The generated program is **not yet a complete playable GBA game**. Runtime dispatch, hardware emulation and instruction coverage are still being expanded.

## Architecture

```text
                         GBA ROM (.gba)
                              │
                              ▼
                 ┌────────────────────────┐
                 │     gba-recompiler     │
                 │                        │
                 │ ARM / Thumb decoder    │
                 │ Reachable discovery    │
                 │ CFG / basic blocks     │
                 │ GBA IR                 │
                 │ Rust code generation   │
                 └────────────┬───────────┘
                              │
                              ▼
                     Generated Rust
                              │
                              ▼
                 ┌────────────────────────┐
                 │      gba-runtime       │
                 │                        │
                 │ CPU state              │
                 │ Memory / cartridge     │
                 │ PPU / APU foundations │
                 │ Timing / runtime API   │
                 │ Save devices           │
                 └────────────┬───────────┘
                              │
                              ▼
                         Native Rust
```

The separation is intentional:

- **`gba-recompiler`** understands ROM code and produces generated Rust.
- **Generated Rust** represents statically recovered game logic.
- **`gba-runtime`** provides the hardware-facing services required by generated code.
- **`gba-cli`** is the development harness for analyzing ROMs and producing generated source.

A graphical frontend is planned, but **egui/eframe is not currently part of the workspace**.

## Workspace

```text
crates/
├── gba-recompiler/
│   └── ARM/Thumb decoding, CFG recovery, IR and Rust code generation
│
├── gba-runtime/
│   └── CPU, memory, cartridge/save and runtime services
│
└── gba-cli/
    └── ROM analysis and generated-source development harness
```

The workspace is defined in `Cargo.toml` and uses Rust 2021. The release profile is configured for thin LTO, a single codegen unit and `panic = "abort"`.

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the architectural contract and roadmap.

## Static recompiler

The recompiler is the central execution path of the project.

### 1. Decode

The decoder currently understands a growing subset of ARM and Thumb instructions and represents each decoded instruction with its address, size, mode and operation.

### 2. Discover reachable code

Analysis starts from an explicit ROM entry address and instruction mode. The analyzer follows statically known successors while keeping ARM and Thumb modes in the CFG key.

```text
entry
  │
  ├── sequential successor
  ├── conditional branch target
  └── unconditional branch target
```

Unknown instructions and dynamic branch-exchange targets terminate static discovery rather than inventing an unsafe successor.

### 3. Recover basic blocks

Leaders are identified from the entry point and branch successors. Sequential instructions remain in the same block until a control-flow boundary is reached.

Each block contains:

- a stable `BlockId`;
- its starting address and ARM/Thumb mode;
- decoded instructions;
- one corresponding IR instruction per decoded instruction;
- statically known successor blocks.

The CFG validation pass checks instruction ownership, address continuity, block identity and successor references.

### 4. Lower to IR

The current IR includes operations such as:

- `Mov`
- `Add`
- `Sub`
- `Cmp`
- `Load`
- `Store`
- `Branch`
- `BranchExchange`
- `Nop`
- `Unknown`

The IR is intentionally explicit about values, addresses and control-flow operations so later optimization passes can operate without depending directly on decoder representations.

### 5. Generate Rust

`gba-recompiler` emits deterministic Rust source containing one generated function per recovered basic block. Generated operations access the runtime through APIs such as register state, memory reads/writes, condition evaluation and dispatch.

The generated source is currently an intermediate development artifact rather than a complete standalone executable game binary.

## Runtime

`gba-runtime` is the hardware boundary consumed by generated code.

Current foundations include:

- ARM7TDMI register state (`r[0..15]`), CPSR and Thumb state;
- 240×160 framebuffer storage and frame counter;
- APU state foundation;
- cartridge ROM storage;
- memory reads/writes through a runtime bus abstraction;
- little-endian 32-bit memory access;
- basic condition-code evaluation;
- cycle/tick accounting;
- runtime hooks for generated-code dispatch and unsupported instructions.

The runtime is intentionally independent of the source ROM and the future frontend.

### Hardware roadmap

The runtime still needs substantial implementation before broad game compatibility:

- complete GBA memory map and I/O registers;
- full ARM7TDMI semantics;
- PPU modes 0–5, sprites and windows;
- APU channels and audio output;
- DMA;
- timers;
- interrupt controller and scheduler;
- keypad/input;
- complete cartridge protocols and timing behavior.

## Cartridge saves

Battery-backed cartridge storage is modeled as part of the cartridge rather than as an emulator savestate.

Supported save models currently include:

```text
SRAM 32 KiB
Flash 64 KiB
Flash 128 KiB
EEPROM 512 B
EEPROM 8 KiB
```

The intended persistence model is:

```text
Generated game code
       │
       ▼
 Cartridge save device
       │
       ▼
    SaveRam
       │
       ├── <game>.sav
       └── <game>.sav.bak
```

Save behavior includes:

- ROM-based save-type detection using known cartridge signatures;
- dirty tracking on writes;
- loading existing save data when its size matches the detected device;
- temporary-file replacement when flushing;
- preservation of the previous `.sav` as `.sav.bak` when possible;
- final flush on `SaveRam` drop.

Save files are kept outside the ROM itself so the source ROM remains immutable.

## CLI

The development CLI accepts an optional ROM path. If none is supplied, it uses the development FireRed ROM path:

```bash
cargo run -p gba-cli --release
```

With an explicit ROM:

```bash
cargo run -p gba-cli --release -- \
  "roms/1636 - Pokemon Fire Red (U)(Squirrels).gba"
```

The CLI currently:

1. reads the ROM;
2. starts static analysis at `0x0800_0000` in ARM mode;
3. reports the recovered entry, block and instruction counts;
4. writes generated Rust to `target/gba_generated.rs`;
5. initializes a runtime and loads the cartridge/save model.

It does **not yet launch a complete generated game execution loop**.

## Test ROMs

Development ROMs currently present under `roms/` include:

- `1636 - Pokemon Fire Red (U)(Squirrels).gba`
- `1986 - Pokemon Emerald (U)(TrashMan).gba`

These ROMs are development/test inputs. Ensure that your use of any ROM complies with applicable copyright and ownership rules.

## Development

Prerequisites:

- Rust stable toolchain
- Cargo

Run the workspace validation locally with:

```bash
cargo fmt
cargo test --workspace
cargo check --workspace --all-targets
```

GitHub Actions runs the same core checks on pushes and pull requests:

```text
cargo fmt
cargo test --workspace
cargo check --workspace --all-targets
```

## Optimization strategy

Optimization is deliberately staged behind correctness and deterministic analysis:

1. Expand correct ARM/Thumb decoding.
2. Make CFG and function discovery robust across real GBA control flow.
3. Strengthen the typed IR and side-effect model.
4. Add constant propagation and dead-code elimination.
5. Specialize safe memory accesses.
6. Add basic-block linking and branch-target specialization.
7. Let Rust/LLVM optimize generated code.
8. Add runtime fast paths for hot hardware operations.
9. Build deterministic regression and benchmark suites.

The guiding principle is to perform as much work as possible at **compile time**, whenever the ROM makes that information statically recoverable.

## Roadmap

The near-term roadmap is:

- [x] Establish Rust workspace and layer separation.
- [x] ARM/Thumb decoder foundation.
- [x] Reachable CFG recovery.
- [x] ARM/Thumb-aware block partitioning.
- [x] Initial typed IR.
- [x] Initial Rust code generator.
- [x] Cartridge save-device model.
- [ ] Expand ARM/Thumb instruction coverage toward real game code.
- [ ] Recover functions and call/return relationships robustly.
- [ ] Add IR optimization passes.
- [ ] Replace generated dispatch placeholders with linked block execution.
- [ ] Implement the complete GBA memory/hardware contract.
- [ ] Add deterministic ROM regression tests.
- [ ] Add frontend/runtime integration.
- [ ] Benchmark generated native code against the reference/runtime path.

## Project philosophy

`gba-rust` favors:

- **static analysis over runtime interpretation**;
- **explicit boundaries between generated code and hardware services**;
- **deterministic transformations**;
- **correctness before aggressive optimization**;
- **small, testable Rust components**;
- **native-code execution as the eventual performance target**.

The project should therefore be understood as an actively developed **static GBA recompilation system**, not as a finished general-purpose GBA emulator.

## License

The workspace is currently licensed under **MIT**. See the repository license for the authoritative terms.
