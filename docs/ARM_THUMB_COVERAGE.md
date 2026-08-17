# ARM/Thumb decoder coverage

This branch expands the GBA recompiler decoder around the ARM7TDMI instruction classes used by GBA software.

## ARM

The decoder now classifies:

- data-processing operations (`AND`/`EOR`/`SUB`/`RSB`/`ADD`/`ADC`/`SBC`/`RSC`/`TST`/`TEQ`/`CMP`/`CMN`/`ORR`/`MOV`/`BIC`/`MVN`);
- multiply and multiply-long forms;
- single data transfers;
- halfword/signed transfers;
- block data transfers;
- swap;
- branch and linked branch;
- `BX`/`BLX` register exchange;
- `MRS`/`MSR`;
- software interrupt;
- coprocessor data, transfer and register-transfer classes.

## Thumb

The decoder now classifies:

- shifted register moves;
- add/subtract register and immediate forms;
- immediate moves/adds/subtracts;
- the complete 16-operation Thumb ALU family;
- high-register operations;
- PC-relative loads;
- register-offset loads/stores;
- signed/halfword register-offset transfers;
- immediate byte/word transfers;
- halfword transfers;
- SP-relative transfers;
- address generation from PC/SP;
- SP adjustment;
- push/pop;
- multiple load/store;
- conditional branches;
- software interrupt;
- unconditional branches;
- `BX`;
- 32-bit Thumb `BL`.

## Semantic boundary

Decoder coverage and execution semantics are deliberately separate. Extended instruction classes are preserved as explicit decoder variants and currently cross the existing IR boundary as `Unknown` until their precise CPU/runtime effects are implemented. This prevents unsupported execution semantics from being silently guessed.

## Regression coverage

The decoder test suite exercises representative encodings from every major ARM and Thumb class and keeps branch/exchange decoding covered alongside the existing CFG and function-discovery tests.
