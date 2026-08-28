# Phase E — Generated execution performance

Phase E runs on `feat/phase-e-generated-execution` after Phase D cartridge/external-memory completion.

## E0 — Deterministic baseline — COMPLETE

`gba-cli/src/bin/phase-e-dispatch-benchmark.rs` provides a reproducible host-side benchmark for generated execution. It reports total time, steps and CFG-membership probes.

## E1 — Static linking — COMPLETE

Statically proven `GeneratedBlockExit::Continue` transitions retain architectural alignment validation while skipping redundant runtime CFG-membership probes. Runtime-resolved `Dynamic` targets continue to validate CFG membership, and BIOS-provided dynamic targets remain on the dynamic path.

Regression coverage locks this distinction down and the benchmark reports zero CFG probes for the static-link transition path.

## E2 — Direct generated-block linking — COMPLETE

`gba-runtime::run_generated_linked` executes a `GeneratedLinkedBlock` entry and follows `LinkedBlockExit::Next { block, ... }` directly through a function pointer. This removes address/mode lookup between statically linked successors while retaining alignment checks at the link boundary.

The generated-link primitive is deliberately independent from the generic `RuntimeContract`: the generic dispatcher remains available for dynamic targets and compatibility boundaries.

## E3 — Block chaining / hot-path dispatch reduction — COMPLETE

`LinkedBlockExit::Next` carries the next generated block itself, so a static hot path stays inside the generated-block loop instead of re-entering the address-based dispatcher on every successor. The chain is deterministic and explicit; exceptions, returns and halts terminate the chain at architectural boundaries.

The direct-link benchmark exercises a multi-step chain rather than a one-shot call, so the hot-path dispatch reduction is measured over sustained block execution.

## E4 — Runtime boundary minimization — COMPLETE

The direct-link path avoids the generic `run_generated_contract` membership callback on every static transition. Architectural alignment remains checked, while dynamic target validation, exception entry, return semantics and timing remain explicit.

No memory, timing, ARM/Thumb, IRQ or exception contract was weakened to obtain the optimization.

## Benchmark comparison

The Phase E benchmark compares two equivalent sustained paths:

- `contract`: generic generated execution with CFG-membership validation.
- `linked`: direct generated-block chaining with zero CFG-membership probes.

It prints `ns/step` for both paths and a `direct_link_speedup` ratio. Absolute host timings are treated as machine-dependent; the structural invariants (same step count and zero linked-path probes) are deterministic.

## Acceptance gate

Phase E is accepted only when `cargo fmt --check`, `cargo check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and the deterministic benchmark contract all pass.
