# gba-rust — Phase F Roadmap

> Phase F is the real-ROM validation phase of `gba-rust`.
>
> Its purpose is to prove, incrementally and measurably, that a real GBA ROM can be statically analyzed, statically recompiled into generated Rust, and then executed against the deterministic GBA runtime without turning the project into an instruction interpreter.
>
> Phase F is intentionally experimental and requires human validation at several milestones. A real ROM may expose architectural, timing, compiler, runtime, or observability gaps that cannot be predicted reliably from isolated unit tests.

## Phase F — Real Game Execution

### F0 — Experimental execution harness — **COMPLETED**

Goal: establish a reproducible path from a real ROM to generated execution and make every failure observable.

- [x] Define the canonical real-ROM execution command and environment contract.
- [x] Accept a user-provided ROM path without embedding commercial ROMs in the repository.
- [x] Validate cartridge metadata before execution.
- [x] Run the existing static analysis / CFG / code-generation pipeline on the ROM.
- [x] Start the generated program against the `gba-runtime` contract.
- [x] Record execution termination reason instead of treating all exits as crashes.
- [x] Record ROM identity and architectural execution state.
- [x] Add deterministic generated-block tracing suitable for comparing repeated runs.
- [x] Validate the F0 path locally against the selected FireRed reference ROM.

**Human validation completed:**

- [x] User supplied and locally validated the reference FireRed ROM.
- [x] User ran the canonical real-ROM command successfully.
- [x] User confirmed the F0 harness runs successfully.

**Acceptance gate:** **PASSED.**

A real ROM can be loaded, statically analyzed, generated and launched through the intended execution path, with enough telemetry to identify the first architectural divergence.

---

### F1 — Deterministic real-ROM boot — **IN PROGRESS**

Goal: prove that the generated program can execute the ROM's reset/initialization path deterministically and establish a meaningful boot checkpoint.

- [x] Establish a fixed 4096-generated-block boot window.
- [x] Capture an architectural checkpoint containing generated steps, PC, ARM/Thumb state, SP, cycles and exit reason.
- [x] Repeat the same boot scenario and compare checkpoints deterministically.
- [x] Compare generated-block traces across repeated boot runs.
- [x] Confirm execution progresses beyond the cartridge entry point.
- [ ] Validate cartridge reset state against the intended boot behavior.
- [ ] Validate the full reset/initialization path used by the selected ROM.
- [ ] Validate ARM/Thumb state transitions against the expected boot path.
- [ ] Validate supervisor/IRQ exception entry used by the boot sequence.
- [ ] Validate early memory initialization against the expected boot behavior.
- [ ] Validate the first timer/interrupt activity encountered by the ROM.
- [ ] Distinguish compiler/code-generation divergence from runtime/hardware divergence at the boot milestone.

**Current automated evidence:**

For the selected FireRed reference ROM, the local F1 test completed two deterministic 4096-generated-block runs and produced the same checkpoint/trace:

```text
size=16777216
 title=POKEMON FIRE
 game_code=BPRE
 entry_target=0x08000204
 steps=4096
 pc=0x081dc81c
 thumb=true
 sp=0x03007e08
 cycles=22132
 exit=StepLimitExceeded
```

This demonstrates deterministic progression through generated execution, but it does not by itself prove that the checkpoint is the intended visual/game boot milestone.

**Human validation required:**

- [ ] User identifies the expected visual or execution milestone for FireRed's boot phase.
- [ ] User compares the first observable output against a trusted reference emulator or hardware behavior.
- [ ] User confirms whether `PC=0x081dc81c`, `SP=0x03007e08`, Thumb state and `22132` cycles represent a genuine boot milestone or an execution plateau.

**Acceptance gate:**

The same ROM reaches the same validated boot checkpoint repeatedly without nondeterministic divergence, and the execution path is still driven by generated blocks rather than instruction-by-instruction interpretation.

F1 must not be marked complete solely because the test process exits successfully or because the checkpoint is deterministic.

---

### F2 — Real exception, BIOS and IRQ execution

Goal: validate that asynchronous hardware and architectural exceptions cooperate correctly with generated control flow.

- [ ] Validate BIOS SWI execution encountered by the selected ROM.
- [ ] Validate supervisor-mode entry and return semantics required by the ROM.
- [ ] Validate IRQ entry at generated block boundaries.
- [ ] Validate banked register preservation across IRQ entry/return.
- [ ] Validate SPSR/CPSR restoration on architectural exception returns.
- [ ] Validate nested or reentrant exception paths when the ROM exercises them.
- [ ] Validate HALT/IntrWait-style waiting paths encountered by the ROM.
- [ ] Validate dynamic exception-vector handling when the vector belongs to generated code.
- [ ] Record exception and IRQ traces with source cycle and generated-block identity.

**Human validation required:**

- [ ] User confirms that the ROM reaches the IRQ/BIOS milestone without external intervention beyond the agreed test procedure.
- [ ] User compares the trace against an expected hardware/reference behavior where available.

**Acceptance gate:**

The ROM can cross real exception/IRQ boundaries without losing generated control flow, CPU mode state, or deterministic resume state.

---

### F3 — Timing, timers, DMA and PPU under real execution

Goal: prove that the central runtime scheduler remains correct when driven continuously by a real ROM.

- [ ] Validate timer progression over sustained generated execution.
- [ ] Validate timer overflow and IRQ request timing.
- [ ] Validate DMA requests and completion boundaries exercised by the ROM.
- [ ] Validate DMA/IRQ interaction under real scheduler pressure.
- [ ] Validate PPU scanline/HBlank/VBlank progression needed by the ROM.
- [ ] Validate scheduler event ordering when multiple events coincide.
- [ ] Validate that generated CPU execution advances the central machine clock correctly.
- [ ] Detect scheduler stalls, runaway event loops and impossible time regressions.
- [ ] Record per-frame timing/event summaries.

**Human validation required:**

- [ ] User confirms visible frame progression or another externally observable timing milestone.
- [ ] User reports any visual/timing discrepancy against reference hardware/reference emulator.

**Acceptance gate:**

The ROM advances through repeated scheduler boundaries without timing deadlock, event-order instability or progressive desynchronization.

---

### F4 — Sustained multi-frame execution

Goal: move from "the ROM boots" to "the ROM lives".

- [ ] Execute a real ROM for a fixed number of complete frames.
- [ ] Execute the same frame window repeatedly and compare deterministic checkpoints.
- [ ] Track generated-block execution volume over time.
- [ ] Track runtime boundary crossings per frame.
- [ ] Track IRQ, DMA, timer and PPU event rates per frame.
- [ ] Detect state drift, memory corruption and execution stalls.
- [ ] Establish the first sustained-execution baseline on the chosen reference ROM.
- [ ] Preserve the baseline trace as a regression artifact outside CI when ROM licensing prevents repository storage.

**Human validation required:**

- [ ] User observes the sustained execution window and confirms whether the ROM is progressing normally.
- [ ] User supplies the next expected interaction/visual milestone.

**Acceptance gate:**

The selected ROM executes for a predefined multi-frame window with deterministic progression and no unexplained architectural drift.

---

### F5 — First deterministic title-screen milestone

Goal: reach a concrete game-visible state, not merely successful code execution.

- [ ] Define a title-screen or equivalent canonical milestone for the selected ROM.
- [ ] Identify the execution evidence proving that the milestone was reached.
- [ ] Validate graphics/memory activity required by the milestone.
- [ ] Validate DMA/PPU activity required by the milestone.
- [ ] Validate audio state only to the extent required by the milestone.
- [ ] Capture a deterministic state/trace around the milestone.
- [ ] Re-run from a clean start and reproduce the milestone.

**Human validation required:**

- [ ] User confirms visually that the expected title screen or equivalent milestone is correct.
- [ ] User provides the expected next interaction milestone.

**Acceptance gate:**

A real ROM reaches a recognizable, deterministic game-visible milestone through the static-recompiled execution path.

---

### F6 — Interactive execution

Goal: establish the first real user-driven interaction loop.

- [ ] Integrate a deterministic input injection path for tests.
- [ ] Validate keypad/input MMIO state transitions.
- [ ] Inject a minimal approved input sequence.
- [ ] Verify that generated game code reacts to the input.
- [ ] Validate the resulting frame progression.
- [ ] Validate that input timing is attached to the central runtime clock rather than an unrelated host clock.
- [ ] Reproduce the same input sequence and compare checkpoints.
- [ ] Record the interaction sequence as a replayable regression scenario.

**Human validation required:**

- [ ] User chooses the minimal interaction sequence that is meaningful for the selected ROM.
- [ ] User observes and confirms the resulting game state.
- [ ] User identifies the next interaction milestone before implementation proceeds.

**Acceptance gate:**

A real ROM accepts deterministic input and produces the expected game-state progression without leaving the static-recompiled execution model.

---

### F7 — Real-ROM compatibility matrix

Goal: turn the single-ROM experiment into a reproducible compatibility program.

- [ ] Define a small set of legally testable ROMs representing different runtime behaviors.
- [ ] Classify each result by boot, frame progression, title/visible milestone, input and sustained execution.
- [ ] Record the first failing boundary for each ROM.
- [ ] Separate failures into compiler, generated-control-flow, runtime, device, timing and host-integration categories.
- [ ] Keep ROM binaries outside CI unless redistribution rights explicitly permit inclusion.
- [ ] Store portable traces, manifests and test procedures instead of copyrighted ROM data.
- [ ] Add a compatibility report format that can be updated without changing the architecture.

**Human validation required:**

- [ ] User selects the next ROM after each compatibility milestone.
- [ ] User validates the expected behavior for every new ROM before results are classified as failures.

**Acceptance gate:**

The project has a reproducible compatibility matrix with clear evidence for both successes and failures, and no result is marked successful solely because the process terminated cleanly.

---

### F8 — Performance characterization after stability

Goal: measure real-ROM performance only after correctness is sufficiently stable.

- [ ] Measure generated instructions/blocks and linked transitions over sustained execution.
- [ ] Measure runtime boundary frequency.
- [ ] Measure scheduler event density.
- [ ] Measure host time per generated step and per frame.
- [ ] Identify hot generated paths from real workloads.
- [ ] Identify excessive runtime crossings caused by missing specialization.
- [ ] Compare the Phase E linked execution path against the remaining dynamic dispatch paths.
- [ ] Optimize only where a real-ROM profile demonstrates a bottleneck.
- [ ] Re-run all deterministic checkpoints after every performance change.

**Human validation required:**

- [ ] User confirms whether measured performance is sufficient for the current milestone.
- [ ] User decides whether the next iteration should prioritize fidelity, observability or performance.

**Acceptance gate:**

Performance work is data-driven by real-ROM execution and does not weaken deterministic behavior or move instruction execution into an interpreter-style runtime loop.

---

## Phase F global engineering rules

### Static recompilation boundary

- The ROM is decoded and analyzed statically.
- Control flow and generated blocks remain the primary execution model.
- The runtime provides dynamic GBA architectural state and hardware effects.
- The runtime must not become the normal instruction-by-instruction ARM/Thumb interpreter.
- Any reference CPU implementation must remain clearly separated from the production generated execution path.

### Determinism

- The central scheduler remains the source of machine time.
- Real-ROM tests must be reproducible from the same ROM, configuration and input sequence.
- Host wall-clock time must not silently become the architectural timing source.
- Every new milestone should have an observable checkpoint or trace.

### Debuggability

- Every real-ROM failure must identify the last successful architectural boundary.
- Traces should correlate cycles, generated blocks, runtime boundaries and hardware events.
- Prefer a narrow, explainable failure over silent fallback behavior.
- Do not hide unsupported instructions, devices or exceptional paths behind fake success.

### Human-in-the-loop policy

Phase F is intentionally not fully automated.

The user must participate whenever a milestone depends on:

- choosing or validating the reference ROM;
- confirming expected visual/game behavior;
- selecting the next interaction milestone;
- distinguishing a genuine hardware/reference discrepancy from an implementation bug;
- deciding whether fidelity, observability or performance should be prioritized next.

Automation should establish evidence. Human validation decides whether the evidence corresponds to the intended real-world milestone.

### Phase F completion criteria

Phase F is complete only when the project can demonstrate, with reproducible evidence, that:

- [ ] at least one real GBA ROM executes through the static-recompilation pipeline;
- [ ] generated Rust blocks remain the primary CPU execution mechanism;
- [ ] the runtime provides the required dynamic hardware contract without becoming the primary instruction interpreter;
- [ ] execution reaches a meaningful game-visible milestone;
- [ ] deterministic multi-frame execution is demonstrated;
- [ ] deterministic input-driven progression is demonstrated;
- [ ] a compatibility matrix records the scope and limits of real-ROM support;
- [ ] performance measurements are based on real workloads;
- [ ] all known limitations are explicit rather than hidden behind unsupported-path fallbacks.

> Phase F completion does not mean "all GBA games work". It means the static recompiler has crossed the boundary from architectural validation into reproducible real-ROM execution with a documented compatibility envelope.
