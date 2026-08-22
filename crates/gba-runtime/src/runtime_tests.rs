use super::*;
use crate::bios::{BiosResult, BiosSwi, HALTCNT, IE, IF, IME, IRQ_HBLANK, IRQ_VBLANK};
use crate::cpu::{CpuMode, ExceptionKind, CPSR_C, CPSR_N, CPSR_V, CPSR_Z, REG_LR, REG_PC, REG_SP};

#[test]
fn compare_updates_all_nzcv_flags() {
    let mut runtime = Runtime::new();
    runtime.compare(0x7fff_ffff, u32::MAX);
    assert!(runtime.cpu.cpsr & CPSR_N != 0);
    assert!(runtime.cpu.cpsr & CPSR_C == 0);
    assert!(runtime.cpu.cpsr & CPSR_V != 0);
}

#[test]
fn condition_contract_covers_signed_and_unsigned_relations() {
    let mut runtime = Runtime::new();
    runtime.cpu.cpsr = CPSR_V | CpuMode::System as u32;
    assert!(runtime.condition_code(6));
    assert!(!runtime.condition_code(7));
    runtime.cpu.cpsr = CPSR_C | CpuMode::System as u32;
    assert!(runtime.condition_code(2));
    assert!(runtime.condition_code(8));
    assert!(!runtime.condition_code(9));
}

#[test]
fn mov_preserves_c_and_v_when_setting_logical_flags() {
    let mut runtime = Runtime::new();
    runtime.cpu.cpsr = CPSR_C | CPSR_V | CpuMode::System as u32;
    runtime.mov(0, 0, true);
    assert!(runtime.cpu.cpsr & CPSR_Z != 0);
    assert!(runtime.cpu.cpsr & CPSR_C != 0);
    assert!(runtime.cpu.cpsr & CPSR_V != 0);
}

#[test]
fn adc_and_sbc_consume_the_current_carry_bit() {
    let mut runtime = Runtime::new();
    runtime.cpu.cpsr = CPSR_C | CpuMode::System as u32;
    runtime.adc(0, 1, 1, true);
    assert_eq!(runtime.read_reg(0), 3);
    runtime.cpu.cpsr = CPSR_C | CpuMode::System as u32;
    runtime.sbc(1, 3, 1, true);
    assert_eq!(runtime.read_reg(1), 2);
    runtime.cpu.cpsr &= !CPSR_C;
    runtime.adc(2, 1, 1, true);
    assert_eq!(runtime.read_reg(2), 2);
    runtime.cpu.cpsr &= !CPSR_C;
    runtime.sbc(3, 3, 1, true);
    assert_eq!(runtime.read_reg(3), 1);
}

#[test]
fn instruction_context_exposes_architectural_pc_and_link_value() {
    let mut runtime = Runtime::new();
    runtime.enter_instruction(0x0800_0100, false);
    assert_eq!(runtime.read_reg(REG_PC), 0x0800_0108);
    runtime.link_from_instruction(0x0800_0100, 4, false);
    assert_eq!(runtime.read_reg(REG_LR), 0x0800_0104);
    runtime.enter_instruction(0x0800_0100, true);
    assert_eq!(runtime.read_reg(REG_PC), 0x0800_0104);
    runtime.link_from_instruction(0x0800_0100, 4, true);
    assert_eq!(runtime.read_reg(REG_LR), 0x0800_0105);
}

#[test]
fn word_reads_apply_arm_unaligned_rotation() {
    let mut runtime = Runtime::new();
    runtime.write32(0x0400_0000, 0x4433_2211);
    assert_eq!(runtime.read32(0x0400_0001), 0x1144_3322);
    assert_eq!(runtime.read32(0x0400_0002), 0x2211_4433);
}

#[test]
fn banked_modes_keep_distinct_stack_and_link_registers() {
    let mut runtime = Runtime::new();
    runtime.write_reg(REG_SP, 0x1000);
    runtime.write_reg(REG_LR, 0x2000);
    runtime.switch_mode(CpuMode::Supervisor);
    runtime.write_reg(REG_SP, 0x3000);
    runtime.write_reg(REG_LR, 0x4000);
    runtime.switch_mode(CpuMode::System);
    assert_eq!(runtime.read_reg(REG_SP), 0x1000);
    assert_eq!(runtime.read_reg(REG_LR), 0x2000);
    runtime.switch_mode(CpuMode::Supervisor);
    assert_eq!(runtime.read_reg(REG_SP), 0x3000);
    assert_eq!(runtime.read_reg(REG_LR), 0x4000);
}

#[test]
fn swi_exception_saves_cpsr_and_restores_banks() {
    let mut runtime = Runtime::new();
    runtime.enter_instruction(0x0800_0100, false);
    runtime.write_reg(REG_SP, 0x1000);
    runtime.write_reg(REG_LR, 0x2000);
    let old = runtime.cpu.cpsr;
    let (vector, thumb) = runtime.raise_exception(ExceptionKind::SoftwareInterrupt);
    assert_eq!((vector, thumb), (0x08, false));
    assert_eq!(runtime.mode(), CpuMode::Supervisor);
    runtime.write_reg(REG_SP, 0x3000);
    runtime.write_reg(REG_LR, 0x4000);
    assert_eq!(runtime.cpu.spsr(), Some(old));
    let result = runtime
        .exception_return(0x0800_0104)
        .expect("exception return");
    assert_eq!(result, (0x0800_0104, false));
    assert_eq!(runtime.mode(), CpuMode::System);
    assert_eq!(runtime.read_reg(REG_SP), 0x1000);
    assert_eq!(runtime.read_reg(REG_LR), 0x2000);
}

#[test]
fn generated_engine_dispatches_iteratively_without_recursive_calls() {
    let mut runtime = Runtime::new();
    let result = runtime.run_generated(0x0800_0000, false, Some(10_000), |rt, address, thumb| {
        rt.tick(1);
        if rt.cycles == 10_000 {
            Err("done")
        } else {
            Ok((address, thumb))
        }
    });
    assert_eq!(result, Err("done"));
    assert_eq!(runtime.cycles, 10_000);
}

#[test]
fn exchange_target_for_dispatch_updates_thumb_and_pc() {
    let mut runtime = Runtime::new();
    assert_eq!(
        runtime.exchange_target_for_dispatch(0x0800_0101),
        (0x0800_0100, true)
    );
    assert!(runtime.cpu.thumb);
    assert_eq!(runtime.read_reg(REG_PC), 0x0800_0100);
}

#[test]
fn mmio_interrupt_registers_are_backed_by_runtime_state() {
    let mut runtime = Runtime::new();
    runtime.write16(IE, IRQ_VBLANK);
    runtime.write16(IME, 1);
    runtime.request_interrupt(IRQ_VBLANK);
    assert_eq!(runtime.read16(IE), IRQ_VBLANK);
    assert_eq!(runtime.read16(IF), IRQ_VBLANK);
    assert_eq!(runtime.mode(), CpuMode::Irq);
    assert_eq!(runtime.read_reg(REG_PC), 0x18);
}

#[test]
fn mmio_if_write_acknowledges_only_requested_bits() {
    let mut runtime = Runtime::new();
    runtime.interrupts.iflags = IRQ_VBLANK | IRQ_HBLANK;
    runtime.write16(IF, IRQ_VBLANK);
    assert_eq!(runtime.read16(IF), IRQ_HBLANK);
}

#[test]
fn halt_and_stop_mmio_change_runtime_power_state() {
    let mut runtime = Runtime::new();
    runtime.write8(HALTCNT, 0);
    assert_eq!(runtime.power, PowerState::Halted);

    runtime.request_interrupt(IRQ_VBLANK);
    assert_eq!(runtime.power, PowerState::Halted);

    runtime.write16(IE, IRQ_VBLANK);
    runtime.request_interrupt(IRQ_VBLANK);
    assert_eq!(runtime.power, PowerState::Running);

    runtime.write8(HALTCNT, 0x80);
    assert_eq!(runtime.power, PowerState::Stopped);
}

#[test]
fn bios_swi_updates_runtime_power_and_memory() {
    let mut runtime = Runtime::new();
    runtime.iwram[0x0100] = 0xaa;
    runtime.cpu.r[0] = 2;
    let result = runtime.bios_swi(BiosSwi::RegisterRamReset);
    assert_eq!(result, BiosResult::RETURNED);
    assert_eq!(runtime.iwram[0x0100], 0);
    runtime.bios_swi(BiosSwi::Halt);
    assert_eq!(runtime.power, PowerState::Halted);
}

#[test]
fn vblank_frame_request_reaches_the_integrated_irq_path() {
    let mut runtime = Runtime::new();
    runtime.write16(IE, IRQ_VBLANK);
    runtime.write16(IME, 1);
    runtime.frame();
    assert_eq!(runtime.mode(), CpuMode::Irq);
    assert_eq!(runtime.read_reg(REG_PC), 0x18);
}

#[test]
fn thumb_bios_swi_restores_thumb_state_and_next_instruction() {
    let mut runtime = Runtime::new();
    runtime.enter_instruction(0x0800_0100, true);
    let result = runtime.bios_swi(BiosSwi::RegisterRamReset);
    assert_eq!(result, BiosResult::RETURNED);
    assert_eq!(runtime.mode(), CpuMode::System);
    assert!(runtime.cpu.thumb);
    assert_eq!(runtime.read_reg(REG_PC), 0x0800_0102);
}

#[test]
fn pending_irq_enters_irq_mode_and_can_be_returned_through_the_contract() {
    let mut runtime = Runtime::new();
    runtime.enter_instruction(0x0800_0300, false);
    runtime.bios_swi(BiosSwi::Halt);
    runtime.write16(IE, IRQ_VBLANK);
    runtime.write16(IME, 1);
    runtime.interrupts.request(IRQ_VBLANK);

    assert_eq!(runtime.deliver_pending_interrupt(), Some((0x18, false)));
    assert_eq!(runtime.mode(), CpuMode::Irq);
    assert_eq!(runtime.power, PowerState::Running);
    assert_eq!(runtime.cpu.spsr(), Some(CpuMode::Supervisor as u32));

    let result = runtime
        .return_from_exception(runtime.read_reg(REG_LR))
        .expect("IRQ handler must be able to return");
    assert_eq!(result, (0x08, false));
    assert_eq!(runtime.mode(), CpuMode::Supervisor);
    assert_eq!(runtime.power, PowerState::Running);
}

#[test]
fn stopped_runtime_does_not_deliver_pending_interrupts() {
    let mut runtime = Runtime::new();
    runtime.write8(HALTCNT, 0x80);
    runtime.write16(IE, IRQ_VBLANK);
    runtime.write16(IME, 1);
    runtime.interrupts.request(IRQ_VBLANK);
    assert_eq!(runtime.deliver_pending_interrupt(), None);
}
