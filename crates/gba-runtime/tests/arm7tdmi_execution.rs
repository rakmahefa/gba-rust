use gba_runtime::{Runtime, REG_LR, REG_PC};

const CPSR_N: u32 = 1 << 31;
const CPSR_Z: u32 = 1 << 30;
const CPSR_C: u32 = 1 << 29;
const CPSR_V: u32 = 1 << 28;

#[test]
fn arm_data_processing_preserves_architectural_flags() {
    let mut rt = Runtime::new();
    rt.write_reg(1, 0x7fff_ffff);
    rt.write_reg(2, 1);
    rt.execute_arm_instruction(0xE091_0002 | (1 << 20));
    assert_eq!(rt.read_reg(0), 0x8000_0000);
    assert_ne!(rt.cpu.cpsr & CPSR_N, 0);
    assert_ne!(rt.cpu.cpsr & CPSR_V, 0);
    assert_eq!(rt.cpu.cpsr & CPSR_C, 0);
}

#[test]
fn arm_adc_and_sbc_consume_carry_and_borrow() {
    let mut rt = Runtime::new();
    rt.cpu.cpsr |= CPSR_C;
    rt.write_reg(0, 10);
    rt.execute_arm_instruction(0xE2A0_0001);
    assert_eq!(rt.read_reg(0), 12);

    rt.write_reg(1, 10);
    rt.write_reg(2, 4);
    rt.cpu.cpsr |= CPSR_C;
    rt.execute_arm_instruction(0xE0C1_2002 | (1 << 20));
    assert_eq!(rt.read_reg(2), 6);
}

#[test]
fn arm_operand2_supports_rrx_and_register_shifts() {
    let mut rt = Runtime::new();
    rt.cpu.cpsr |= CPSR_C;
    rt.write_reg(1, 1);
    rt.execute_arm_instruction(0xE1B0_0061 | (1 << 20));
    assert_eq!(rt.read_reg(0), 0x8000_0000);
    assert_ne!(rt.cpu.cpsr & CPSR_C, 0);

    rt.write_reg(1, 1);
    rt.write_reg(3, 2);
    rt.execute_arm_instruction(0xE1A0_0311);
    assert_eq!(rt.read_reg(0), 4);
}

#[test]
fn arm_multiply_and_long_multiply_write_expected_parts() {
    let mut rt = Runtime::new();
    rt.write_reg(1, 3);
    rt.write_reg(2, 7);
    rt.execute_arm_instruction(0xE000_0291);
    assert_eq!(rt.read_reg(0), 21);

    rt.write_reg(1, u32::MAX);
    rt.write_reg(2, 2);
    rt.execute_arm_instruction(0xE083_0291);
    assert_eq!(rt.read_reg(0), 0xffff_fffe);
    assert_eq!(rt.read_reg(3), 1);
}

#[test]
fn arm_single_transfer_and_unaligned_word_follow_bus_rules() {
    let mut rt = Runtime::new();
    rt.write32(0x0400_0000, 0x4433_2211);
    rt.write_reg(1, 0x0400_0000);
    rt.execute_arm_instruction(0xE591_0000);
    assert_eq!(rt.read_reg(0), 0x4433_2211);

    rt.write_reg(1, 0x0400_0000);
    rt.execute_arm_instruction(0xE5D1_0001);
    assert_eq!(rt.read_reg(0), 0x22);

    rt.write_reg(1, 0x0400_0001);
    rt.execute_arm_instruction(0xE591_0000);
    assert_eq!(rt.read_reg(0), 0x1144_3322);
}

#[test]
fn arm_halfword_signed_loads_are_sign_extended() {
    let mut rt = Runtime::new();
    rt.write16(0x0400_0010, 0x80ff);
    rt.write8(0x0400_0020, 0x80);
    rt.write_reg(1, 0x0400_0010);
    rt.execute_arm_instruction(0xE1D1_00B0);
    assert_eq!(rt.read_reg(0), 0x0000_80ff);
    rt.write_reg(1, 0x0400_0020);
    rt.execute_arm_instruction(0xE1D1_00D0);
    assert_eq!(rt.read_reg(0), 0xffff_ff80);
}

#[test]
fn arm_block_transfer_updates_registers_and_writeback() {
    let mut rt = Runtime::new();
    rt.write_reg(0, 0x1111_1111);
    rt.write_reg(2, 0x2222_2222);
    rt.write_reg(1, 0x0400_0100);
    rt.execute_arm_instruction(0xE8A1_0005);
    assert_eq!(rt.read_reg(1), 0x0400_0108);

    rt.write_reg(0, 0);
    rt.write_reg(2, 0);
    rt.write_reg(1, 0x0400_0100);
    rt.execute_arm_instruction(0xE8B1_0005);
    assert_eq!(rt.read_reg(0), 0x1111_1111);
    assert_eq!(rt.read_reg(2), 0x2222_2222);
}

#[test]
fn arm_mrs_reads_cpsr_and_msr_updates_selected_fields() {
    let mut rt = Runtime::new();
    rt.cpu.cpsr = CPSR_N | CPSR_C;
    rt.execute_arm_instruction(0xE10F_0000);
    assert_eq!(rt.read_reg(0), CPSR_N | CPSR_C);

    rt.write_reg(0, CPSR_Z | CPSR_V);
    rt.execute_arm_instruction(0xE128_F000);
    assert_eq!(rt.cpu.cpsr & 0xF000_0000, CPSR_Z | CPSR_V);
}

#[test]
fn thumb_core_alu_and_stack_operations_match_architecture() {
    let mut rt = Runtime::new();
    rt.execute_thumb_instruction(0x2001);
    assert_eq!(rt.read_reg(0), 1);
    rt.execute_thumb_instruction(0x3002);
    assert_eq!(rt.read_reg(0), 3);

    rt.write_reg(13, 0x0400_0200);
    rt.write_reg(0, 0x1234_5678);
    rt.write_reg(REG_LR, 0x0800_0101);
    rt.execute_thumb_instruction(0xB501);
    assert_eq!(rt.read_reg(13), 0x0400_01F8);
    rt.write_reg(0, 0);
    rt.execute_thumb_instruction(0xBD01);
    assert_eq!(rt.read_reg(0), 0x1234_5678);
    assert_eq!(rt.read_reg(13), 0x0400_0200);
}

#[test]
fn thumb_high_register_move_to_pc_preserves_thumb_state() {
    let mut rt = Runtime::new();
    rt.set_thumb(true);
    rt.write_reg(0, 0x0800_0101);
    let result = rt.execute_thumb_instruction(0x4687).expect("MOV PC,R0 must produce a control transfer");
    assert_eq!(result, (0x0800_0100, true));
    assert_eq!(rt.read_reg(REG_PC), 0x0800_0100);
    assert!(rt.cpu.thumb);
}
