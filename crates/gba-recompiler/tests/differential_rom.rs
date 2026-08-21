use gba_recompiler::{analyze, build_semantic_program, discover_functions, generate_semantic, Mode, ROM_BASE};
use gba_runtime::{Runtime, REG_PC};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Reference {
    r0: u32,
    cpsr: u32,
    pc: u32,
}

fn reference_mov_add_cmp(initial: Reference) -> Reference {
    let mut state = initial;
    state.r0 = 1;
    state.r0 = state.r0.wrapping_add(2);
    let result = state.r0.wrapping_sub(3);
    let carry = state.r0 >= 3;
    let overflow = ((state.r0 ^ 3) & (state.r0 ^ result) & 0x8000_0000) != 0;
    state.cpsr &= !(gba_runtime::CPSR_N | gba_runtime::CPSR_Z | gba_runtime::CPSR_C | gba_runtime::CPSR_V);
    if result & 0x8000_0000 != 0 {
        state.cpsr |= gba_runtime::CPSR_N;
    }
    if result == 0 {
        state.cpsr |= gba_runtime::CPSR_Z;
    }
    if carry {
        state.cpsr |= gba_runtime::CPSR_C;
    }
    if overflow {
        state.cpsr |= gba_runtime::CPSR_V;
    }
    state.pc = state.pc.wrapping_add(12);
    state
}

#[test]
fn rom_fixture_matches_independent_reference_model() {
    let words = [
        0xE3A0_0001u32, // mov r0, #1
        0xE280_0002u32, // add r0, r0, #2
        0xE350_0003u32, // cmp r0, #3
    ];
    let mut rom = Vec::new();
    for word in words {
        rom.extend_from_slice(&word.to_le_bytes());
    }

    let program = analyze(&rom, ROM_BASE, Mode::Arm).expect("ROM fixture must decode");
    assert_eq!(program.cfg.blocks.len(), 1);
    assert_eq!(program.cfg.blocks[0].instructions.len(), 3);

    let functions = discover_functions(&program);
    let semantic = build_semantic_program(&program, &functions).expect("semantic lowering must succeed");
    let generated = generate_semantic(&program, &semantic, "fixture_entry");
    assert!(generated.source.contains("rt.mov(0"));
    assert!(generated.source.contains("rt.add(0"));
    assert!(generated.source.contains("rt.compare"));

    let mut runtime = Runtime::new();
    runtime.write_reg(REG_PC, ROM_BASE);
    for word in words {
        runtime.execute_arm_instruction(word);
        runtime.write_reg(REG_PC, runtime.read_reg(REG_PC).wrapping_add(4));
    }

    let actual = Reference {
        r0: runtime.read_reg(0),
        cpsr: runtime.cpu.cpsr,
        pc: runtime.read_reg(REG_PC),
    };
    let expected = reference_mov_add_cmp(Reference {
        r0: 0,
        cpsr: runtime.cpu.cpsr & !(gba_runtime::CPSR_N | gba_runtime::CPSR_Z | gba_runtime::CPSR_C | gba_runtime::CPSR_V),
        pc: ROM_BASE,
    });

    assert_eq!(actual.r0, expected.r0);
    assert_eq!(
        actual.cpsr & (gba_runtime::CPSR_N | gba_runtime::CPSR_Z | gba_runtime::CPSR_C | gba_runtime::CPSR_V),
        expected.cpsr & (gba_runtime::CPSR_N | gba_runtime::CPSR_Z | gba_runtime::CPSR_C | gba_runtime::CPSR_V)
    );
    assert_eq!(actual.pc, ROM_BASE + 12);
}
