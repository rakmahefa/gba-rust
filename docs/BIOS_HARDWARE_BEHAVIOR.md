# BIOS / Hardware Behavior Contract

This document defines the observable GBA BIOS behavior modeled by `gba-runtime` without embedding the proprietary BIOS binary.

## Runtime integration

The BIOS model is now part of `Runtime` rather than an isolated helper layer. `Runtime` owns:

- CPU power state (`Running`, `Halted`, `Stopped`);
- interrupt state (`IE`, `IF`, `IME`);
- EWRAM, IWRAM, palette RAM, VRAM and OAM backing storage;
- key MMIO register state such as `WAITCNT`, `POSTFLG`, `KEYINPUT` and `DISPSTAT`.

Generated code and CPU-side memory accesses therefore reach the same state machine through `Runtime::read8/read16/read32` and `Runtime::write8/write16/write32`.

## Scope

The model currently covers the hardware-facing BIOS calls needed to bootstrap a real GBA execution environment:

- `SWI 00h` — SoftReset
- `SWI 01h` — RegisterRamReset
- `SWI 02h` — Halt
- `SWI 03h` — Stop
- `SWI 04h` — IntrWait
- `SWI 05h` — VBlankIntrWait
- IRQ enable (`IE`), request/acknowledge (`IF`) and master enable (`IME`)
- CPU power states: running, halt and stop
- GBA BIOS/WRAM/display-memory address regions and key control-register addresses

The implementation is behavioral: it models the externally observable state transitions instead of distributing BIOS machine code through the runtime.

## MMIO behavior

The runtime treats the following addresses as hardware-backed instead of ordinary dictionary-backed I/O:

```text
DISPSTAT   0x04000004
KEYINPUT   0x04000130
IE         0x04000200
IF         0x04000202
WAITCNT    0x04000204
IME        0x04000208
POSTFLG    0x04000300
HALTCNT    0x04000301
```

`IF` is write-one-to-clear: writing a set bit acknowledges that interrupt request, while IRQ entry does not implicitly clear the request latch.

`HALTCNT` changes `Runtime::power`: a value without bit 7 enters `Halted`, while bit 7 enters `Stopped`. An enabled interrupt request wakes a halted runtime, and an eligible pending IRQ then vectors through the ARM7TDMI IRQ exception path.

## Reset behavior

`SoftReset` reads the boot flag from `IWRAM[0x7FFA]` before resetting CPU state. It then establishes the canonical GBA stacks:

```text
SVC  0x03007FE0
IRQ  0x03007FA0
SYS  0x03007F00
```

and enters ARM system mode. A zero boot flag targets `0x08000000`; any non-zero value targets `0x02000000`.

## Selective RAM reset

`RegisterRamReset` uses `r0` as a bitmask. The model clears the corresponding runtime backing storage for:

```text
bit 0  EWRAM
bit 1  IWRAM, excluding the BIOS-reserved top 0x200 bytes
bit 2  palette RAM
bit 3  VRAM
bit 4  OAM
bit 5  interrupt/SIO control state represented by the model
bit 7  interrupt master-enable state
```

The top `0x200` bytes of IWRAM are deliberately preserved because BIOS/system bookkeeping lives there.

## Wait and power behavior

`Halt` transitions the CPU into `Halted`. Video, audio, timers, serial and other hardware are expected to continue advancing; the runtime therefore treats halt as an execution state rather than a global shutdown.

`Stop` transitions into `Stopped`, a stronger low-power state. The integrated runtime refuses normal generated execution while stopped until a future wake-source model explicitly changes that state.

`IntrWait` tests the requested interrupt mask and returns immediately when a matching `IE & IF` bit is already pending. Otherwise the runtime enters the halted state. The model keeps BIOS waiting semantics separate from CPU-level IRQ eligibility, so `IntrWait` does not itself fabricate an IRQ entry.

`VBlankIntrWait` is the VBlank-specialized form and waits on `IRQ_VBLANK`.

## IRQ entry

When `IME` is enabled, the corresponding bit is enabled in `IE`, and a bit is pending in `IF`, `Runtime::service_interrupts` enters IRQ mode through `service_pending_irq`. CPSR is preserved in `SPSR_irq`, IRQs are masked, ARM state is selected, `LR_irq` receives the architectural return point, and the CPU vectors to `0x00000018`.

The BIOS IRQ vector itself remains outside this behavioral model. This separation keeps the runtime independent from a redistributable BIOS binary.

The runtime also connects the existing frame path to `IRQ_VBLANK`: a completed `Runtime::frame()` requests VBlank through the same interrupt controller used by MMIO and BIOS waits.

## Memory-map anchors

The runtime exports and directly backs the canonical regions used by the hardware model:

```text
BIOS      0x00000000..0x00003FFF  behavioral access only
EWRAM     0x02000000..0x0203FFFF
IWRAM     0x03000000..0x03007FFF
I/O       0x04000000..0x040003FF
Palette   0x05000000..0x050003FF
VRAM      0x06000000..0x06017FFF
OAM       0x07000000..0x070003FF
```

## Design rule

Do not add a BIOS image to the source tree. Future BIOS work should extend the behavioral contract with additional SWIs, protection semantics, timing observations and regression fixtures while keeping the hardware behavior owned by `Runtime`.
