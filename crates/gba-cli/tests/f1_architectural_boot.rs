use gba_runtime::{Cartridge, CpuMode, Runtime, IRQ_TIMER0};
use gba_runtime::cpu::{REG_LR, REG_PC, REG_SP};
use gba_runtime::timers::{CONTROL_ENABLE, CONTROL_IRQ};

const CARTRIDGE_BASE: u32 = 0x0800_0000;
const SYSTEM_SP: u32 = 0x0300_7f00;
const IRQ_SP: u32 = 0x0300_7fa0;
const SVC_SP: u32 = 0x0300_7fe0;
const IRQ_VECTOR: u32 = 0x0000_0018;

fn boot_runtime() -> Runtime {
    let mut runtime = Runtime::new();
    runtime.load_cartridge(Cartridge::from_rom(vec![0; 0x200], "phase-f1-test"));
    runtime
}

#[test]
fn f1_cartridge_reset_contract_matches_gba_boot_boundary() {
    let runtime = boot_runtime();

    assert_eq!(runtime.cpu.mode(), CpuMode::System);
    assert!(!runtime.cpu.thumb);
    assert_eq!(runtime.cpu.r[REG_PC], CARTRIDGE_BASE);
    assert_eq!(runtime.cpu.r[REG_SP], SYSTEM_SP);
    assert_eq!(runtime.cpu.banked.user_system_sp_lr[0], SYSTEM_SP);
    assert_eq!(runtime.cpu.banked.irq_sp_lr[0], IRQ_SP);
    assert_eq!(runtime.cpu.banked.svc_sp_lr[0], SVC_SP);
    assert_eq!(runtime.cycles, 0);
    assert_eq!(runtime.scheduler.now(), 0);
}

#[test]
fn f1_first_timer_overflow_is_raised_on_the_central_machine_clock() {
    let mut runtime = boot_runtime();
    runtime.interrupts.ie = IRQ_TIMER0;
    runtime.interrupts.ime = true;
    runtime.timers[0].write_reload(u16::MAX);
    runtime.timers[0].write_control(CONTROL_ENABLE | CONTROL_IRQ);

    runtime.advance_cycles(1);

    assert_eq!(runtime.cycles, 1);
    assert_eq!(runtime.scheduler.now(), 1);
    assert_eq!(runtime.timers[0].counter(), u16::MAX);
    assert_ne!(runtime.interrupts.iflags & IRQ_TIMER0, 0);
}

#[test]
fn f1_first_timer_irq_enters_irq_mode_at_the_architectural_boundary() {
    let mut runtime = boot_runtime();
    runtime.cpu.set_thumb(true);
    runtime.cpu.r[REG_PC] = 0x0800_1001;
    runtime.interrupts.ie = IRQ_TIMER0;
    runtime.interrupts.ime = true;
    runtime.timers[0].write_reload(u16::MAX);
    runtime.timers[0].write_control(CONTROL_ENABLE | CONTROL_IRQ);

    runtime.advance_cycles(1);
    assert!(runtime.service_interrupts());

    assert_eq!(runtime.cpu.mode(), CpuMode::Irq);
    assert!(!runtime.cpu.thumb);
    assert_eq!(runtime.cpu.r[REG_PC], IRQ_VECTOR);
    assert_eq!(runtime.cpu.r[REG_LR], 0x0800_1001);
    assert_eq!(runtime.cpu.banked.irq_sp_lr[0], IRQ_SP);
    assert_ne!(runtime.cpu.spsr().unwrap() & (1 << 5), 0);
}

#[test]
fn f1_generated_dispatch_samples_a_pending_timer_irq_before_the_next_block() {
    let mut runtime = boot_runtime();
    runtime.cpu.set_thumb(true);
    runtime.cpu.r[REG_PC] = 0x0800_1000;
    runtime.interrupts.ie = IRQ_TIMER0;
    runtime.interrupts.ime = true;
    runtime.timers[0].write_reload(u16::MAX);
    runtime.timers[0].write_control(CONTROL_ENABLE | CONTROL_IRQ);
    runtime.advance_cycles(1);

    let result = runtime.run_generated(0x0800_1000, true, Some(1), |runtime, address, thumb| {
        assert_eq!(runtime.cpu.mode(), CpuMode::Irq);
        assert_eq!(address, IRQ_VECTOR);
        assert!(!thumb);
        Err("phase-f1 irq vector reached")
    });

    assert_eq!(result, Err("phase-f1 irq vector reached"));
}
