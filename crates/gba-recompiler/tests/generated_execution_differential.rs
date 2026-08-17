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
        Self {
            regs: [0; 16],
            cpsr: gba_runtime::CpuMode::System as u32,
            thumb: false,
            memory: BTreeMap::new(),
            steps: 0,
        }
    }

    fn set_nzcv(&mut self, result: u32, carry: bool, overflow: bool) {
        self.cpsr &= !(CPSR_N | CPSR_Z | CPSR_C | CPSR_V);
        if result & 0x8000_0000 != 0 {
            self.cpsr |= CPSR_N;
        }
        if result == 0 {
            self.cpsr |= CPSR_Z;
        }
        if carry {
            self.cpsr |= CPSR_C;
        }
        if overflow {
            self.cpsr |= CPSR_V;
        }
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

    fn str_word(&mut self, rn: usize, rd: usize) {
        let address = self.regs[rn];
        for (offset, byte) in self.regs[rd].to_le_bytes().into_iter().enumerate() {
            self.memory.insert(address + offset as u32, byte);
        }
    }

    fn ldr_word(&mut self, rn: usize, rd: usize) {
        let address = self.regs[rn];
        let bytes = [
            *self.memory.get(&address).unwrap_or(&0),
            *self.memory.get(&(address + 1)).unwrap_or(&0),
            *self.memory.get(&(address + 2)).unwrap_or(&0),
            *self.memory.get(&(address + 3)).unwrap_or(&0),
        ];
        self.regs[rd] = u32::from_le_bytes(bytes);
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
            0x0340_0000 => self.cmp_imm(rn, imm),
            _ => match raw {
                0xE581_0000 => self.str_word(rn, rd),
                0xE591_2000 => self.ldr_word(rn, rd),
                0xEAFF_FFFE => return Some((ROM_BASE, false)),
                _ => panic!("reference model does not support ARM instruction {raw:#010x}"),
            },
        }
        self.steps += 1;
        None
    }

    fn execute_linear(&mut self, words: &[u32]) {
        for &word in words {
            assert!(self.apply_arm(word).is_none(), "linear fixture must not branch");
        }
        self.regs[15] = ROM_BASE + words.len() as u32 * 4;
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

fn assert_architectural_state_matches_reference(
    actual: &ArchitecturalState,
    reference: &ReferenceState,
) {
    assert_eq!(actual.registers, reference.regs);
    assert_eq!(actual.cpsr, reference.cpsr);
    assert_eq!(actual.thumb, reference.thumb);
    assert_eq!(actual.cycles, reference.steps);
}

fn execute_linear_with_runtime(words: &[u32]) -> (Runtime, gba_runtime::GeneratedExecutionResult) {
    let mut runtime = Runtime::new();
    let result = runtime
        .run_generated_contract(
            ROM_BASE,
            false,
            Some(32),
            |rt, address, thumb| {
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
            },
            |address, thumb| {
                !thumb && address >= ROM_BASE && address < ROM_BASE + words.len() as u32 * 4
            },
        )
        .expect("fixture execution");
    (runtime, result)
}

#[test]
fn generated_execution_matches_independent_reference_for_linear_arm_fixture() {
    let words = fixture_words(include_str!("fixtures/linear_arm.hex"));
    let program = analyze(&arm_rom(&words), ROM_BASE, Mode::Arm).expect("fixture analysis");
    let functions = discover_functions(&program);
    let semantic = gba_recompiler::build_semantic_program(&program, &functions)
        .expect("semantic fixture");
    let generated = gba_recompiler::generate_semantic(&program, &semantic, "fixture_entry");

    assert!(generated.source.contains("run_generated_contract"));
    assert!(generated.source.contains("fn dispatch_block"));
    assert!(generated.source.contains("fn is_linked_block"));

    let (runtime, result) = execute_linear_with_runtime(&words);
    let mut reference = ReferenceState::new();
    reference.execute_linear(&words);

    assert_eq!(
        result.exit,
        GeneratedExecutionExit::Halted {
            address: ROM_BASE + words.len() as u32 * 4,
            thumb: false
        }
    );
    assert_eq!(result.steps, words.len() as u64);
    assert_architectural_state_matches_reference(&result.state, &reference);
    assert_eq!(result.state.registers[0], 2);
    assert!(result.state.cpsr & CPSR_Z != 0);
    assert_eq!(runtime.read_reg(gba_runtime::REG_PC), result.state.pc());
}

#[test]
fn generated_execution_matches_memory_effects_in_rom_fixture() {
    let words = fixture_words(include_str!("fixtures/memory_roundtrip_arm.hex"));
    let program = analyze(&arm_rom(&words), ROM_BASE, Mode::Arm).expect("memory fixture analysis");
    let functions = discover_functions(&program);
    let semantic = gba_recompiler::build_semantic_program(&program, &functions)
        .expect("memory semantic fixture");
    let generated = gba_recompiler::generate_semantic(&program, &semantic, "memory_fixture");

    assert!(generated.source.contains("fn dispatch_block"));
    assert!(generated.source.contains("rt.write32(address & !3, rt.read_reg(0));"));
    assert!(generated.source.contains("rt.read32(address)"));
    assert!(!generated.source.contains("execute_arm_instruction"));
    assert!(!generated.source.contains("execute_thumb_instruction"));

    let (runtime, result) = execute_linear_with_runtime(&words);
    let mut reference = ReferenceState::new();
    reference.execute_linear(&words);

    assert_architectural_state_matches_reference(&result.state, &reference);
    assert_eq!(result.state.registers[0], 0x2a);
    assert_eq!(result.state.registers[2], 0x2a);
    assert!(result.state.cpsr & CPSR_Z != 0);

    for (&address, &expected) in &reference.memory {
        assert_eq!(runtime.read8(address), expected, "memory mismatch at {address:#x}");
    }
}