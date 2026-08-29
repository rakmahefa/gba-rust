# Phase F0 — Experimental real-ROM execution harness

Phase F0 establishes the first reproducible path from a user-provided GBA ROM to **statically generated Rust execution**.

The harness is deliberately not an emulator loop. The ROM is analyzed statically, a Rust program is generated from the recovered CFG, and that generated program executes against `gba-runtime`.

## Canonical invocation

Set `GBA_REAL_ROM` to a legally available local ROM and run:

```bash
GBA_REAL_ROM="/path/to/game.gba" \
  cargo test -p gba-cli --test real_rom_execution -- --nocapture
```

A workspace-relative path is also accepted.

For the executable development harness, the equivalent path is:

```bash
cargo run -p gba-cli -- /path/to/game.gba --execute --max-steps 512
```

## F0 execution boundary

```text
user ROM
  │
  ▼
cartridge header validation
  │
  ▼
static CFG analysis
  │
  ▼
Rust code generation
  │
  ▼
temporary generated Rust runner
  │
  ▼
gba-runtime
  │
  ▼
GeneratedBlockExit / GeneratedExecutionExit
```

The normal CPU path remains generated blocks. The runtime provides architectural state, memory and hardware effects; it does not become the production ARM/Thumb instruction interpreter.

## F0 evidence

The validation test records:

- ROM byte size;
- cartridge title;
- game code;
- validated cartridge entry target;
- generated execution step count;
- architectural PC and ARM/Thumb state;
- SP and machine cycles;
- generated execution termination reason;
- generated block transition trace.

The test runs the same generated runner twice with the same configuration and compares the generated trace. A differing trace is a deterministic-execution failure and must be investigated before proceeding to later Phase F milestones.

## Human validation gate

F0 still requires a real ROM supplied by the user and a user-confirmed expected boot/termination milestone. CI remains ROM-free: commercial ROM binaries are never required to be committed to the repository.

## F0 acceptance

F0 is considered established when a legally available local ROM can be:

1. validated as a GBA cartridge;
2. statically analyzed into a CFG;
3. translated into generated Rust;
4. executed through the generated-block runtime boundary;
5. observed through a structured execution result and trace; and
6. repeated without trace divergence.

A clean process exit without these observations is not considered evidence of successful real-ROM execution.
