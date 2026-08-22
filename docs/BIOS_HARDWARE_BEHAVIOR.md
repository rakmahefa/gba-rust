# BIOS / Hardware Behavior Contract

This document defines the observable GBA BIOS behavior modeled by `gba-runtime` without embedding the proprietary BIOS binary.

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

`Stop` transitions into `Stopped`, a stronger low-power state intended to pause most of the system until an allowed wake source occurs.

`IntrWait` tests the requested interrupt mask and returns immediately when a matching `IE & IF` bit is already pending. Otherwise the CPU enters the halted state. The model does not force `IME` on merely because `IntrWait` was called; BIOS waiting semantics and hardware interrupt eligibility remain separate concerns.

`VBlankIntrWait` is the VBlank-specialized form and waits on `IRQ_VBLANK`.

## IRQ entry

When `IME` is enabled, the corresponding bit is enabled in `IE`, and a bit is pending in `IF`, `service_pending_irq` enters IRQ mode, preserves CPSR in `SPSR_irq`, masks IRQs, switches to ARM state, stores the architectural return point in `LR_irq`, and vectors to `0x00000018`.

The BIOS IRQ vector itself remains outside this behavioral model. This separation keeps the runtime independent from a redistributable BIOS binary.

## Memory-map anchors

The runtime exports the canonical regions used by the hardware model:

```text
BIOS      0x00000000..0x00003FFF
EWRAM     0x02000000..0x0203FFFF
IWRAM     0x03000000..0x03007FFF
I/O       0x04000000..0x040003FF
Palette   0x05000000..0x050003FF
VRAM      0x06000000..0x06017FFF
OAM       0x07000000..0x070003FF
```

Key registers exported by the model include `IE`, `IF`, `WAITCNT`, `IME`, `POSTFLG`, `HALTCNT`, `KEYINPUT` and `DISPSTAT`.

## Design rule

Do not add a BIOS image to the source tree. Future BIOS work should extend the behavioral contract with additional SWIs, protection semantics, timing observations and regression fixtures.
