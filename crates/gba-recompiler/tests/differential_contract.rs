use gba_recompiler::{analyze, build_semantic_program, discover_functions, generate_semantic, IrOp, Mode, ROM_BASE};
use gba_runtime::{ArchitecturalState, Runtime, RuntimeContract, CPSR_C, CPSR_N, CPSR_V, CPSR_Z};

fn arm_rom(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn reference_add(lhs: u32, rhs: u32, carry: bool) -> (u32, [bool; 4]) {
    let wide = lhs as u64 + rhs as u64 + carry as u64;
    let result = wide as u32;
    let n = result & 0x8000_0000 != 0;
    let z = result == 0;
    let c = wide >> 32 != 0;
    let v = ((lhs ^ !rhs) & (lhs ^ result) & 0x8000_0000) != 0;
    (result, [n, z, c, v])
}

fn reference_sub(lhs: u32, rhs: u32, borrow: bool) -> (u32, [bool; 4]) {
    let result = lhs.wrapping_sub(rhs).wrapping_sub(borrow as u32);
    let n = result & 0x8000_0000 != 0;
    let z = result == 0;
    let c = (lhs as u64) >= (rhs as u64 + borrow as u64);
    let v = ((lhs ^ rhs) & (lhs ^ result) & 0x8000_0000) != 0;
    (result, [n, z, c, v])
}

fn assert_nzcv(state: &ArchitecturalState, flags: [bool; 4]) {
    assert_eq!(state.cpsr & CPSR_N != 0, flags[0]);
    assert_eq!(state.cpsr & CPSR_Z != 0, flags[1]);
    assert_eq!(state.cpsr & CPSR_C != 0, flags[2]);
    assert_eq!(state.cpsr & CPSR_V != 0, flags[3]);
}

#[test]
fn arm_add_and_sub_match_independent_reference_model() {
    let cases = [
        (0u32, 0u32, false),
        (1, 2, false),
        (u32::MAX, 1, false),
        (0x7fff_ffff, 1, false),
        (0x8000_0000, 0x8000_0000, false),
        (0xffff_fffe, 1, true),
        (0x1234_5678, 0x8765_4321, true),
    ];

    for &(lhs, rhs, carry) in &cases {
        let mut runtime = Runtime::new();
        runtime.cpu.cpsr = gba_runtime::CpuMode::System as u32 | if carry { CPSR_C } else { 0 };
        let (expected, flags) = reference_add(lhs, rhs, carry);
        if carry {
            runtime.adc(0, lhs, rhs, true);
        } else {
            runtime.add(0, lhs, rhs, true);
        }
        assert_eq!(runtime.read_reg(0), expected, "ADD-family {lhs:#x} + {rhs:#x} carry={carry}");
        assert_nzcv(&runtime.architectural_state(), flags);

        let mut runtime = Runtime::new();
        let borrow = !carry;
        runtime.cpu.cpsr = gba_runtime::CpuMode::System as u32 | if carry { CPSR_C } else { 0 };
        let (expected, flags) = reference_sub(lhs, rhs, borrow);
        runtime.sbc(0, lhs, rhs, true);
        assert_eq!(runtime.read_reg(0), expected, "SBC {lhs:#x} - {rhs:#x} borrow={borrow}");
        assert_nzcv(&runtime.architectural_state(), flags);
    }
}

#[test]
fn decoder_ir_runtime_and_codegen_preserve_instruction_identity() {
    let words = [
        0xE3A0_0001u32, // MOV r0, #1
        0xE280_0001u32, // ADD r0, r0, #1
        0xE240_0001u32, // SUB r0, r0, #1
        0xE350_0001u32, // CMP r0, #1
    ];
    let program = analyze(&arm_rom(&words), ROM_BASE, Mode::Arm).expect("analysis");
    let block = &program.cfg.blocks[0];
    assert_eq!(block.ir.len(), words.len());
    for (index, (ir, raw)) in block.ir.iter().zip(words).enumerate() {
        assert_eq!(ir.source_raw, raw);
        assert_eq!(ir.address, ROM_BASE + (index as u32) * 4);
        assert_eq!(ir.ops.len(), 1);
    }

    assert!(matches!(block.ir[0].ops[0], IrOp::Mov { dst: 0, .. }));
    assert!(matches!(block.ir[1].ops[0], IrOp::Add { dst: 0, lhs: 0, .. }));
    assert!(matches!(block.ir[2].ops[0], IrOp::Sub { dst: 0, lhs: 0, .. }));
    assert!(matches!(block.ir[3].ops[0], IrOp::Cmp { lhs: 0, .. }));

    let functions = discover_functions(&program);
    let semantic = build_semantic_program(&program, &functions).expect("semantic contract");
    let generated = generate_semantic(&program, &semantic, "entry");
    for raw in words {
        assert!(generated.source.contains("rt.enter_instruction"));
        assert!(generated.source.contains(&format!("{raw:#010x}")));
    }
    assert!(generated.source.contains("rt.run_generated"));
    assert!(generated.source.contains("fn dispatch_block"));
}

#[test]
fn generated_dispatch_contract_agrees_with_runtime_target_transitions() {
    let mut runtime = Runtime::new();
    runtime.enter_instruction(ROM_BASE, false);
    let first = RuntimeContract::execute_arm_instruction(&mut runtime, 0xEA00_0000);
    assert_eq!(first, Some((ROM_BASE + 8, false)));
    assert_eq!(runtime.read_reg(gba_runtime::REG_PC), ROM_BASE + 8);
    assert!(!runtime.architectural_state().thumb);

    let mut runtime = Runtime::new();
    runtime.enter_instruction(ROM_BASE, false);
    runtime.write_reg(0, ROM_BASE);
    let first = RuntimeContract::execute_arm_instruction(&mut runtime, 0xE12F_FF10);
    assert_eq!(first, Some((ROM_BASE, false)));
}
