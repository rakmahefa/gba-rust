use gba_runtime::Runtime;

const EWRAM: u32 = 0x0200_0000;
const IWRAM: u32 = 0x0300_0000;

#[test]
fn cpu_set_copies_halfwords_and_restores_exception_state() {
    let mut runtime = Runtime::new();
    runtime.cpu.r[0] = EWRAM;
    runtime.cpu.r[1] = IWRAM;
    runtime.cpu.r[2] = 3;

    runtime.write16(EWRAM, 0x1111);
    runtime.write16(EWRAM + 2, 0x2222);
    runtime.write16(EWRAM + 4, 0x3333);

    let result = runtime
        .execute_bios_swi_comment(0x0b, true)
        .expect("CpuSet must be implemented");

    assert!(result.returned);
    assert_eq!(runtime.read16(IWRAM), 0x1111);
    assert_eq!(runtime.read16(IWRAM + 2), 0x2222);
    assert_eq!(runtime.read16(IWRAM + 4), 0x3333);
    assert_eq!(runtime.mode(), gba_runtime::CpuMode::System);
}

#[test]
fn cpu_set_fill_repeats_a_word_source() {
    let mut runtime = Runtime::new();
    runtime.cpu.r[0] = EWRAM;
    runtime.cpu.r[1] = IWRAM;
    runtime.cpu.r[2] = (1 << 24) | (1 << 26) | 2;

    runtime.write32(EWRAM, 0xdead_beef);

    runtime
        .execute_bios_swi_comment(0x0b, true)
        .expect("CpuSet fill must be implemented");

    assert_eq!(runtime.read32(IWRAM), 0xdead_beef);
    assert_eq!(runtime.read32(IWRAM + 4), 0xdead_beef);
}

#[test]
fn cpu_fast_set_rounds_transfer_length_to_eight_words() {
    let mut runtime = Runtime::new();
    runtime.cpu.r[0] = EWRAM;
    runtime.cpu.r[1] = IWRAM;
    runtime.cpu.r[2] = 2;

    for index in 0..8u32 {
        runtime.write32(EWRAM + index * 4, 0x1000 + index);
    }

    runtime
        .execute_bios_swi_comment(0x0c, true)
        .expect("CpuFastSet must be implemented");

    for index in 0..8u32 {
        assert_eq!(runtime.read32(IWRAM + index * 4), 0x1000 + index);
    }
}
