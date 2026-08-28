# Phase E — Generated execution performance

Phase E starts on `feat/phase-e-generated-execution` after Phase D cartridge/external-memory completion.

## E0 — Baseline — COMPLETE

The branch adds `gba-cli/src/bin/phase-e-dispatch-benchmark.rs`, an explicit host-side benchmark for generated execution. It reports total execution time, steps and CFG-membership probes so later linking work can be compared against a reproducible baseline.

## E1 — Static linking — COMPLETE

Static CFG transitions now use the existing `GeneratedBlockExit::Continue` path as a statically proven edge. The runtime still validates architectural alignment, but no longer performs a redundant CFG-membership probe for that edge.

Runtime-resolved `GeneratedBlockExit::Dynamic` targets continue to require CFG membership validation. BIOS-provided dynamic return targets are emitted through the dynamic path as well. Exception vectors and normal returns retain their existing boundary semantics.

The Phase E benchmark now measures the static-link path and exposes `cfg_membership_probes`; the expected E1 invariant is zero probes for the statically proven transition.

## E2 — Direct block linking

Move from address/mode redispatch toward direct generated-block linkage for safe static edges. IRQ sampling and architectural exception boundaries remain explicit and cannot be bypassed by optimization.

## E3 — Block chaining

Hot static paths can then be chained to reduce generic dispatcher re-entry. Chain construction and invalidation must remain deterministic and must respect boundaries where asynchronous hardware state can become CPU-visible.

## E4 — Runtime boundary minimization

Finally, proven CPU-local paths can reduce generated-to-runtime crossings without weakening memory, timing, exception or ARM/Thumb architectural contracts.

## Acceptance gate

Every Phase E increment is accepted only with `cargo fmt --check`, `cargo check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and a deterministic benchmark comparison where applicable.
