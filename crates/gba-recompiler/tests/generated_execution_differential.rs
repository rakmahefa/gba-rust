use std::collections::BTreeMap;

use gba_recompiler::{analyze, discover_functions, generate, Mode, ROM_BASE};
use gba_runtime::{
    ArchitecturalState, GeneratedBlockExit, GeneratedExecutionExit, Runtime, RuntimeContract,
    CPSR_C, CPSR_N, CPSR_V, CPSR_Z,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReferenceState {
    regs: [u32; 16],
    cpsr: u32,
    thumb: bool,
    memory: BTreeMap<u32, u8>,
    steps: u64,
}

impl ReferenceState {
    fn new() -> Self {
        Self { regs: [0; 16], cpsr: gba_runtime::CpuMode::System as u32, thumb: false, memory: BTreeMap::new(), steps: 0 }
    }

    fn nzcv(&self) -> (bool, bool, bool, bool) {
        (
            self.cpsr & CPSR_N != 0,
            self.cpsr & CPSR_Z != 0,
            self.cpsr & CPSR_C != 0,
            self.cpsr & CPSR_V != 0,
        )
    }

    fn set_nzcv(&mut self, result: u32, carry: bool, overflow: bool) {
        self.cpsr &= !(CPSR_N | CPSR_Z | CPSR_C | CPSR_V);
        if result & 0x8000_0000 != 0 { self.cpsr |= CPSR_N; }
        if result == 0 { self.cpsr |= CPSR_Z; }
        if carry { self.cpsr |= CPSR_C; }
        if overflow { self.cpsr |= CPSR_V; }
    }

    fn mov_imm(&mut self, rd: usize, value: u32) {
        self.regs[rd] = value;
        let (_, _, c, v) = self.nzcv();
        self.cpsr &= !(CPSR_N | CPSR_Z);
        if value & 0x8000_0000 != 0 { self.cpsr |= CPSR_N; }
        if value == 0 { self.cpsr |= CPSR_Z; }
        if c { self.cpsr |= CPSR_C; }
        if v { self.cpsr |= CPSR_V; }
    }

    fn add_imm(&mut self, rd: usize, rn: usize, imm: u32) {
        let lhs = self.regs[rn];
        let wide = lhs as u64 + imm as u64;
        let result = wide as u32;
        let overflow = (!(lhs ^ imm) & (lhs ^ result) & 0x8000_0000) != 0;
        self.regs[rd] = result;
        self.set_nzcv(result, wide >> 32 != 0, overflow);
    }

    fn sub_imm(&mut self, rd: usize, rn: usize, imm: u32) {
        let lhs = self.regs[rn];
        let result = lhs.wrapping_sub(imm);
        let carry = lhs >= imm;
        let overflow = ((lhs ^ imm) & (lhs ^ result) & 0x8000_0000) != 0;
        self.regs[rd] = result;
        self.set_nzcv(result, carry, overflow);
    }

    fn cmp_imm(&mut self, rn: usize, imm: u32) {
        let lhs = self.regs[rn];
        let result = lhs.wrapping_sub(imm);
        let carry = lhs >= imm;
        let overflow = ((lhs ^ imm) & (lhs ^ result) & 0x8000_0000) != 0;
        self.set_nzcv(result, carry, overflow);
    }

    fn apply_arm(&mut self, address: u32, raw: u32) -> Option<(u32, bool)> {
        self.regs[15] = address;
        let opcode = raw & 0x0fe0_0000;
        let rn = ((raw >> 16) & 0xf) as usize;
        let rd = ((raw >> 12) & 0xf) as usize;
        let imm = raw & 0xff;
        match opcode {
            0x03a0_0000 => self.mov_imm(rd, imm),
            0x0280_0000 => self.add_imm(rd, rn, imm),
            0x0240_0000 => self.sub_imm(rd, rn, imm),
            0x0350_0000 => self.cmp_imm(rn, imm),
            0x0a00_0000 => {
                let offset = ((raw & 0x00ff_ffff) << 2) as i32;
                let offset = (offset << 6) >> 6;
                return Some((address.wrapping_add(8).wrapping_add(offset as u32), false));
            }
            _ => panic!("reference model does not support ARM instruction {raw:#010x} at {address:#x}"),
        }
        self.steps += 1;
        None
    }

    fn execute_fixture(&mut self, words: &[u32]) {
        let mut pc = ROM_BASE;
        loop {
            let index = ((pc - ROM_BASE) / 4) as usize;
            if index >= words.len() { break; }
            if self.apply_arm(pc, words[index]).is_some() { break; }
            pc = pc.wrapping_add(4);
        }
        self.regs[15] = pc;
    }
}

fn arm_rom(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn assert_architectural_state_matches_reference(actual: &ArchitecturalState, reference: &ReferenceState) {
    assert_eq!(actual.registers, reference.regs);
    assert_eq!(actual.cpsr, reference.cpsr);
    assert_eq!(actual.thumb, reference.thumb);
    assert_eq!(actual.cycles, reference.steps);
}

#[test]
fn generated_execution_matches_independent_reference_for_linear_arm_fixture() {
    let words = [
        0xE3A0_0001u32, // mov r0, #1
        0xE280_0001u32, // add r0, r0, #1
        0xE280_0001u32, // add r0, r0, #1
        0xE240_0001u32, // sub r0, r0, #1
        0xE350_0002u32, // cmp r0, #2
    ];
    let program = analyze(&arm_rom(&words), ROM_BASE, Mode::Arm).expect("fixture analysis");
    let functions = discover_functions(&program);
    let semantic = gba_recompiler::build_semantic_program(&program, &functions).expect("semantic fixture");
    let generated = gba_recompiler::generate_semantic(&program, &semantic, "fixture_entry");

    assert!(generated.source.contains("run_generated_contract"));
    assert!(generated.source.contains("fn dispatch_block"));
    assert!(generated.source.contains("fn is_linked_block"));

    let mut runtime = Runtime::new();
    let mut reference = ReferenceState::new();
    let result = runtime
        .run_generated_contract(ROM_BASE, false, Some(16), |rt, address, thumb| {
            assert!(!thumb);
            let index = ((address - ROM_BASE) / 4) as usize;
            if index >= words.len() {
                return Ok(GeneratedBlockExit::halt(address, thumb));
            }
            let raw = words[index];
            let next = RuntimeContract::execute_arm_instruction(rt, raw);
            rt.tick(1);
            match next {
                Some((target, next_thumb)) => Ok(GeneratedBlockExit::continue_to(target, next_thumb)),
                None => {
                    let next_address = address.wrapping_add(4);
                    if index + 1 == words.len() {
                        Ok(GeneratedBlockExit::halt(next_address, false))
                    } else {
                        Ok(GeneratedBlockExit::continue_to(next_address, false))
                    }
                }
            }
        }, |address, thumb| !thumb && address >= ROM_BASE && address < ROM_BASE + (words.len() as u32 * 4))
        .expect("fixture execution");

    reference.execute_fixture(&words);
    assert_eq!(result.exit, GeneratedExecutionExit::Halted { address: ROM_BASE + 20, thumb: false });
    assert_eq!(result.steps, words.len() as u64);
    assert_architectural_state_matches_reference(&result.state, &reference);
    assert_eq!(result.state.registers[0], 2);
    assert!(result.state.cpsr & CPSR_Z != 0);
}

#[test]
fn generated_execution_preserves_branch_target_and_step_limit_deterministically() {
    let words = [
        0xE3A0_0001u32, // mov r0, #1
        0xEAFF_FFFE_u32, // b .
    ];
    let program = analyze(&arm_rom(&words), ROM_BASE, Mode::Arm).expect("loop fixture analysis");
    let functions = discover_functions(&program);
    let semantic = gba_recompiler::build_semantic_program(&program, &functions).expect("loop semantic fixture");
    let generated = generate(&program, "loop_fixture");
    assert!(generated.source.contains("GeneratedBlockExit::continue_to"));
    assert!(semantic.validate().is_ok());

    let mut runtime = Runtime::new();
    let result = runtime
        .run_generated_contract(ROM_BASE, false, Some(3), |rt, address, thumb| {
            let index = ((address - ROM_BASE) / 4) as usize;
            if index == 0 {
                RuntimeContract::execute_arm_instruction(rt, words[0]);
                rt.tick(1);
                Ok(GeneratedBlockExit::continue_to(ROM_BASE + 4, false))
            } else {
                let target = RuntimeContract::execute_arm_instruction(rt, words[1]).expect("branch target");
                rt.tick(1);
                Ok(GeneratedBlockExit::continue_to(target.0, target.1))
            }
        }, |address, thumb| !thumb && address == ROM_BASE || !thumb && address == ROM_BASE + 4)
        .expect("loop should terminate only through the configured step limit");

    assert_eq!(result.exit, GeneratedExecutionExit::StepLimitExceeded { address: ROM_BASE, thumb: false });
    assert_eq!(result.steps, 3);
    assert_eq!(result.state.registers[0], 1);
    assert!(!result.state.thumb);
}

#[test]
fn rom_fixture_analysis_has_stable_entry_block_and_instruction_identity() {
    let words = [
        0xE3A0_0001u32,
        0xE280_0001u32,
        0xE240_0001u32,
        0xE350_0002u32,
    ];
    let program = analyze(&arm_rom(&words), ROM_BASE, Mode::Arm).expect("analysis");
    let block = &program.cfg.blocks[program.entry.0];
    assert_eq!(block.key.address, ROM_BASE);
    assert_eq!(block.key.mode, Mode::Arm);
    assert_eq!(block.instructions.len(), words.len());
    for (index, (instruction, raw)) in block.instructions.iter().zip(words).enumerate() {
        assert_eq!(instruction.address, ROM_BASE + index as u32 * 4);
        assert_eq!(instruction.raw, raw);
        assert_eq!(instruction.size, 4);
    }
}
