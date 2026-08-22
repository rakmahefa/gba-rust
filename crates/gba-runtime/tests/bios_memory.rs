use gba_runtime::{BiosLoadError, Runtime, BIOS_SIZE};

#[test]
fn load_bios_maps_bytes_at_zero() {
    let mut runtime = Runtime::new();
    let mut bios = vec![0u8; BIOS_SIZE];
    bios[0] = 0x46;
    bios[1] = 0x42;
    bios[0x3fff] = 0xa5;

    runtime.load_bios(&bios).unwrap();

    assert_eq!(runtime.read8(0x0000_0000), 0x46);
    assert_eq!(runtime.read8(0x0000_0001), 0x42);
    assert_eq!(runtime.read8(0x0000_3fff), 0xa5);
}

#[test]
fn bios_reads_support_all_runtime_widths() {
    let mut runtime = Runtime::new();
    runtime
        .load_bios(
            &[0x78, 0x56, 0x34, 0x12]
                .into_iter()
                .chain(std::iter::repeat(0))
                .take(BIOS_SIZE)
                .collect::<Vec<_>>(),
        )
        .unwrap();

    assert_eq!(runtime.read8(0), 0x78);
    assert_eq!(runtime.read16(0), 0x5678);
    assert_eq!(runtime.read32(0), 0x1234_5678);
}

#[test]
fn bios_is_read_only_on_the_cpu_bus() {
    let mut runtime = Runtime::new();
    runtime.load_bios(&vec![0x7e; BIOS_SIZE]).unwrap();

    runtime.write8(0, 0xff);
    runtime.write16(0, 0xffff);
    runtime.write32(0, 0xffff_ffff);

    assert_eq!(runtime.read8(0), 0x7e);
    assert_eq!(runtime.read16(0), 0x7e7e);
    assert_eq!(runtime.read32(0), 0x7e7e_7e7e);
}

#[test]
fn invalid_bios_does_not_replace_a_valid_loaded_image() {
    let mut runtime = Runtime::new();
    runtime.load_bios(&vec![0x11; BIOS_SIZE]).unwrap();

    let error = runtime.load_bios(&[0x22; 4]).unwrap_err();

    assert_eq!(
        error,
        BiosLoadError::InvalidSize {
            expected: BIOS_SIZE,
            actual: 4,
        }
    );
    assert_eq!(runtime.read8(0), 0x11);
}
