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
    steps: u64,
}

impl ReferenceState {
    fn new() -> Self {
        Self { regs: [0; 16], cpsr: gba_runtime::CpuMode::System as u32, thumb: false, steps: 0 }
    }

    fn set_nzcv(&mut self, result: u32, carry: bool, overflow: bool) {
        self.cpsr &= !(CPSR_N | CPSR_Z | CPSR_C | CPSR_V);
        if result & 0x8000_0000 != 0 { self.cpsr |= CPSR_N; }
        if result == 0 { self.cpsr |= CPSR_Z; }
        if carry { self.cpsr |= CPSR_C; }
        if overflow { self.cpsr |= CPSR_V; }
    }

    fn mov_imm(&mut self, rd: usize, value: u32, set_flags: bool) {
        self.regs[rd] = value;
        if set_flags {
            let carry = self.cpsr & CPSR_C != 0;
            let overflow = self.cpsr & CPSR_V != 0;
            self.set_nzcv(value, carry, overflow);
        }
    }

    fn add_imm(&mut self, rd: usize, rn: usize, imm: u32, set_flags: bool) {
        let lhs = self.regs[rn];
        let wide = lhs as u64 + imm as u64;
        let result = wide as u32;
        self.regs[rd] = result;
        if set_flags {
            let overflow = (!(lhs ^ imm) & (lhs ^ result) & 0x8000_0000) != 0;
            self.set_nzcv(result, wide >> 32 != 0, overflow);
        }
    }

    fn sub_imm(&mut self, rd: usize, rn: usize, imm: u32, set_flags: bool) {
        let lhs = self.regs[rn];
        let result = lhs.wrapping_sub(imm);
        self.regs[rd] = result;
        if set_flags {
            let carry = lhs >= imm;
            let overflow = ((lhs ^ imm) & (lhs ^ result) & 0x8000_0000) != 0;
            self.set_nzcv(result, carry, overflow);
        }
    }

    fn cmp_imm(&mut self, rn: usize, imm: u32) {
        let lhs = self.regs[rn];
        let result = lhs.wrapping_sub(imm);
        let carry = lhs >= imm;
        let overflow = ((lhs ^ imm) & (lhs ^ result) & 0x8000_0000) != 0;
        self.set_nzcv(result, carry, overflow);
    }

    fn apply_arm(&mut self, raw: u32) -> Option<(u32, bool)> {
        let opcode = raw & 0x0fe0_0000;
        let rn = ((raw >> 16) & 0xf) as usize;
        let rd = ((raw >> 12) & 0xf) as usize;
        let imm = raw & 0xff;
        let set_flags = raw & (1 << 20) != 0;

        match opcode {
            0x03a0_0000 => self.mov_imm(rd, imm, set_flags),
            0x0280_0000 => self.add_imm(rd, rn, imm, set_flags),
            0x0240_0000 => self.sub_imm(rd, rn, imm, set_flags),
            0x0350_0000 => self.cmp_imm(rn, imm),
            0x0a00_0000 => {
                let signed_imm24 = ((raw & 0x00ff_ffff) as i32) << 2;
                let offset = signed_imm24 >> 6 << 6;
                return Some((ROM_BASE.wrapping_add(8).wrapping_add(offset as u32), false));
            }
            _ => panic!("reference model does not support ARM instruction {raw:#010x}"),
        }
        self.steps += 1;
        None
    }

    fn execute_linear(&mut self, words: &[u32]) {
        for &word in words {
            self.apply_arm(word).expect_none("linear fixture must not branch");
        }
        self.regs[15] = ROM_BASE + words.len() as u32 * 4;
    }
}

trait OptionExt<T> {
    fn expect_none(self, message: &str);
}

impl<T> OptionExt<T> for Option<T> {
    fn expect_none(self, message: &str) {
        assert!(self.is_none(), "{message}");
    }
}

fn fixture_words(text: &str) -> Vec<u32> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| u32::from_str_radix(line.trim(), 16).expect("valid fixture word"))
        .collect()
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
    let words = fixture_words(include_str!("fixtures/linear_arm.hex"));
    let rom = arm_rom(&words);
    let program = analyze(&rom, ROM_BASE, Mode::Arm).expect("fixture analysis");
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
            rt.enter_instruction(address, false);
            let next = RuntimeContract::execute_arm_instruction(rt, words[index]);
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
        }, |address, thumb| !thumb && address >= ROM_BASE && address < ROM_BASE + words.len() as u32 * 4)
        .expect("fixture execution");

    reference.execute_linear(&words);
    assert_eq!(result.exit, GeneratedExecutionExit::Halted { address: ROM_BASE + words.len() as u32 * 4, thumb: false });
    assert_eq!(result.steps, words.len() as u64);
    assert_architectural_state_matches_reference(&result.state, &reference);
    assert_eq!(result.state.registers[0], 2);
    assert!(result.state.cpsr & CPSR_Z != 0);
}

#[test]
fn generated_execution_preserves_branch_target_and_step_limit_deterministically() {
    let words = fixture_words(include_str!("fixtures/branch_loop_arm.hex"));
    let rom = arm_rom(&words);
    let program = analyze(&rom, ROM_BASE, Mode::Arm).expect("loop fixture analysis");
    let functions = discover_functions(&program);
    let semantic = gba_recompiler::build_semantic_program(&program, &functions).expect("loop semantic fixture");
    let generated = generate(&program, "loop_fixture");
    assert!(generated.source.contains("GeneratedBlockExit::continue_to"));
    gba_recompiler::validate_semantic_program(&program, &functions, &semantic).expect("semantic validation");

    let mut runtime = Runtime::new();
    let result = runtime
        .run_generated_contract(ROM_BASE, false, Some(3), |rt, address, thumb| {
            assert!(!thumb);
            let index = ((address - ROM_BASE) / 4) as usize;
            rt.enter_instruction(address, false);
            let target = RuntimeContract::execute_arm_instruction(rt, words[index]).expect("loop branch target");
            rt.tick(1);
            Ok(GeneratedBlockExit::continue_to(target.0, target.1))
        }, |address, thumb| !thumb && (address == ROM_BASE || address == ROM_BASE + 4))
        .expect("loop should terminate only through the configured step limit");

    assert_eq!(result.exit, GeneratedExecutionExit::StepLimitExceeded { address: ROM_BASE + 4, thumb: false });
    assert_eq!(result.steps, 3);
    assert_eq!(result.state.registers[0], 1);
    assert!(!result.state.thumb);
}

#[test]
fn rom_fixture_analysis_has_stable_entry_block_and_instruction_identity() {
    let words = fixture_words(include_str!("fixtures/linear_arm.hex"));
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
