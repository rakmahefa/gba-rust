use super::Runtime;
use crate::bios::{HALTCNT, IE, IF};
use crate::cpu::CpuMode;
use crate::mmio;

#[test]
fn dispcnt_supports_halfword_and_byte_accesses() {
    let mut runtime = Runtime::new();
    runtime.write16(mmio::DISPCNT, 0xa55a);
    assert_eq!(runtime.read16(mmio::DISPCNT), 0xa552);

    // The initial value has bit 3 clear, so a low-byte write keeps it clear.
    runtime.write8(mmio::DISPCNT, 0x12);
    assert_eq!(runtime.read16(mmio::DISPCNT), 0xa512);
    runtime.write8(mmio::DISPCNT_HI, 0x34);
    assert_eq!(runtime.read16(mmio::DISPCNT), 0x3412);

    // A pre-existing read-only bit must survive both byte and halfword writes.
    runtime.dispcnt = 0x0008;
    runtime.write8(mmio::DISPCNT, 0x00);
    assert_eq!(runtime.read16(mmio::DISPCNT), 0x0008);

    runtime.dispcnt = 0xa508;
    runtime.write8(mmio::DISPCNT_HI, 0xaa);
    assert_eq!(runtime.read16(mmio::DISPCNT), 0xaa08);

    runtime.dispcnt = 0xa508;
    runtime.write16(mmio::DISPCNT, 0x0000);
    assert_eq!(runtime.read16(mmio::DISPCNT), 0x0008);
}

#[test]
fn dispstat_preserves_read_only_status_bits() {
    let mut runtime = Runtime::new();
    runtime.dispstat = mmio::DISPSTAT_STATUS_MASK;
    runtime.write16(mmio::DISPSTAT, 0xffff);
    assert_eq!(
        runtime.dispstat & mmio::DISPSTAT_STATUS_MASK,
        mmio::DISPSTAT_STATUS_MASK
    );
    assert_eq!(
        runtime.dispstat & !mmio::DISPSTAT_STATUS_MASK,
        mmio::DISPSTAT_WRITABLE_MASK
    );
}

#[test]
fn interrupt_registers_reject_reserved_source_bits() {
    let mut runtime = Runtime::new();
    runtime.write16(IE, 0xffff);
    assert_eq!(runtime.read16(IE), mmio::INTERRUPT_SOURCE_MASK);

    runtime.interrupts.iflags = mmio::INTERRUPT_SOURCE_MASK;
    runtime.write16(IF, 0xffff);
    assert_eq!(runtime.read16(IF), 0);
}

#[test]
fn interrupt_register_byte_writes_preserve_all_valid_low_bits() {
    let mut runtime = Runtime::new();
    runtime.write8(IE, 0xff);
    runtime.write8(mmio::IE_HI, 0xff);
    assert_eq!(runtime.read16(IE), mmio::INTERRUPT_SOURCE_MASK);
}

#[test]
fn waitcnt_preserves_the_read_only_gamepak_type_bit_and_ignores_reserved_bit() {
    let mut runtime = Runtime::new();
    runtime.waitcnt = 0x8000;
    runtime.write16(mmio::WAITCNT, 0xffff);
    assert_eq!(runtime.read16(mmio::WAITCNT), 0xdfff);
    assert_eq!(runtime.read16(mmio::WAITCNT) & 0x2000, 0);

    runtime.waitcnt = 0;
    runtime.write16(mmio::WAITCNT, 0xffff);
    assert_eq!(runtime.read16(mmio::WAITCNT), mmio::WAITCNT_WRITABLE_MASK);
}

#[test]
fn read_only_video_and_keypad_registers_ignore_writes() {
    let mut runtime = Runtime::new();
    runtime.vcount = 123;
    runtime.keyinput = 0x02aa;

    runtime.write16(mmio::VCOUNT, 0xffff);
    runtime.write16(mmio::KEYINPUT, 0);

    assert_eq!(runtime.read16(mmio::VCOUNT), 123);
    assert_eq!(runtime.read16(mmio::KEYINPUT), 0x02aa);
}

#[test]
fn postflg_masks_reserved_bits_and_halfword_writes_reach_haltcnt() {
    let mut runtime = Runtime::new();
    runtime.write8(mmio::POSTFLG, 0xff);
    assert_eq!(runtime.read8(mmio::POSTFLG), 1);

    runtime.write16(mmio::POSTFLG, 0x0180);
    assert_eq!(runtime.read8(mmio::POSTFLG), 0);
    assert_eq!(runtime.power, crate::bios::PowerState::Halted);
}

#[test]
fn haltcnt_controls_power_but_is_write_only_on_read() {
    let mut runtime = Runtime::new();
    runtime.write8(HALTCNT, 0x80);
    assert_eq!(runtime.power, crate::bios::PowerState::Stopped);
    assert_eq!(runtime.read8(HALTCNT), 0);
}

#[test]
fn enabling_interrupts_via_mmio_still_enters_irq_at_the_existing_boundary() {
    let mut runtime = Runtime::new();
    runtime.write16(IE, crate::bios::IRQ_VBLANK);
    runtime.write16(mmio::IME, 1);
    runtime.request_interrupt(crate::bios::IRQ_VBLANK);

    assert_eq!(runtime.mode(), CpuMode::Irq);
    assert_eq!(runtime.read_reg(crate::cpu::REG_PC), 0x18);
}
