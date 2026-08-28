# Phase E — Generated execution performance

Phase E starts on `feat/phase-e-generated-execution` after Phase D cartridge/external-memory completion.

## E0 — Baseline

The branch adds `gba-cli/src/bin/phase-e-dispatch-benchmark.rs`, an explicit host-side benchmark for the current generated execution contract. It reports total execution time, steps and CFG-membership probes so later linking work can be compared against a reproducible baseline.

## E1 — Static linking

The first optimization target is the distinction between statically proven CFG transitions and runtime-resolved targets. Static edges must retain architectural alignment validation, but should not repeat a generated-CFG membership lookup that was already established during code generation.

## E2 — Direct block linking

Once the baseline is stable, the dispatcher will move from address/mode redispatch toward direct generated-block linkage for safe static edges. IRQ sampling and architectural exception boundaries remain explicit and cannot be bypassed by optimization.

## E3 — Block chaining

Hot static paths can then be chained to reduce generic dispatcher re-entry. Chain construction and invalidation must remain deterministic and must respect boundaries where asynchronous hardware state can become CPU-visible.

## E4 — Runtime boundary minimization

Finally, proven CPU-local paths can reduce generated-to-runtime crossings without weakening memory, timing, exception or ARM/Thumb architectural contracts.

## Acceptance gate

Every Phase E increment is accepted only with `cargo fmt --check`, `cargo check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and a deterministic benchmark comparison where applicable.
