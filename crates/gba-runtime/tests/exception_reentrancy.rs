use gba_runtime::{CpuMode, ExceptionKind, Runtime, REG_LR, REG_PC, REG_SP};

#[test]
fn nested_irq_preserves_supervisor_exception_state() {
    let mut runtime = Runtime::new();
    runtime.enter_instruction(0x0800_0100, false);
    runtime.switch_mode(CpuMode::Supervisor);
    runtime.write_reg(REG_SP, 0x0300_7fe0);
    runtime.write_reg(REG_LR, 0x0800_0200);
    runtime.cpu.cpsr |= 1 << 7;

    runtime.cpu.set_thumb(false);
    let svc_cpsr = runtime.cpu.cpsr;
    runtime.raise_exception(ExceptionKind::Irq);

    assert_eq!(runtime.mode(), CpuMode::Irq);
    assert_eq!(runtime.cpu.spsr(), Some(svc_cpsr));
    assert_eq!(runtime.read_reg(REG_PC), 0x18);
    assert_eq!(runtime.read_reg(REG_SP), runtime.cpu.banked.irq_sp_lr[0]);

    let irq_lr = runtime.read_reg(REG_LR);
    runtime.exception_return(irq_lr).expect("IRQ return must restore SVC");
    assert_eq!(runtime.mode(), CpuMode::Supervisor);
    assert_eq!(runtime.read_reg(REG_SP), 0x0300_7fe0);
    assert_eq!(runtime.read_reg(REG_LR), 0x0800_0200);
    assert_eq!(runtime.cpu.cpsr, svc_cpsr);
}

#[test]
fn irq_return_restores_thumb_caller_state_and_link_target() {
    let mut runtime = Runtime::new();
    runtime.enter_instruction(0x0800_0300, true);
    let caller_pc = runtime.read_reg(REG_PC);
    let caller_cpsr = runtime.cpu.cpsr;
    runtime.raise_exception(ExceptionKind::Irq);
    let irq_lr = runtime.read_reg(REG_LR);

    assert_eq!(irq_lr, 0x0800_0302);
    let target = irq_lr;
    let restored = runtime.exception_return(target).expect("IRQ return must restore Thumb");
    assert_eq!(restored, (0x0800_0302, true));
    assert_eq!(runtime.mode(), CpuMode::System);
    assert_eq!(runtime.cpu.thumb, true);
    assert_eq!(runtime.read_reg(REG_PC), 0x0800_0302);
    assert_eq!(runtime.cpu.cpsr, caller_cpsr);
    assert_ne!(caller_pc, runtime.read_reg(REG_PC));
}

#[test]
fn exception_return_requires_an_active_spsr() {
    let mut runtime = Runtime::new();
    runtime.write_reg(REG_LR, 0x0800_0100);
    assert!(runtime.exception_return(runtime.read_reg(REG_LR)).is_none());
    assert_eq!(runtime.mode(), CpuMode::System);
    assert_eq!(runtime.read_reg(REG_SP), 0);
}

#[test]
fn generated_cycle_tick_does_not_switch_to_irq_mid_instruction() {
    let mut runtime = Runtime::new();
    runtime.write16(0x0400_0200, 1);
    runtime.write16(0x0400_0208, 1);
    runtime.cpu.enter_instruction_for_test(0x0800_0100, false);
    runtime.interrupts.request(1);
    let mode_before = runtime.mode();
    runtime.tick(1);
    assert_eq!(runtime.mode(), mode_before);
    assert_eq!(runtime.read_reg(REG_PC), 0x0800_0108);
}
