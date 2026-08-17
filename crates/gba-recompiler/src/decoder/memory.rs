use thiserror::Error;

pub const ROM_BASE: u32 = 0x0800_0000;

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("address {0:#x} is outside the cartridge ROM")]
    OutOfRange(u32),
    #[error("truncated instruction at {0:#x}")]
    Truncated(u32),
}

pub fn read_arm(rom: &[u8], address: u32) -> Result<u32, DecodeError> {
    let offset = address
        .checked_sub(ROM_BASE)
        .ok_or(DecodeError::OutOfRange(address))? as usize;
    if offset + 4 > rom.len() {
        return Err(DecodeError::Truncated(address));
    }
    Ok(u32::from_le_bytes(
        rom[offset..offset + 4]
            .try_into()
            .expect("validated four-byte ARM instruction"),
    ))
}

pub fn read_thumb(rom: &[u8], address: u32) -> Result<u16, DecodeError> {
    let offset = address
        .checked_sub(ROM_BASE)
        .ok_or(DecodeError::OutOfRange(address))? as usize;
    if offset + 2 > rom.len() {
        return Err(DecodeError::Truncated(address));
    }
    Ok(u16::from_le_bytes(
        rom[offset..offset + 2]
            .try_into()
            .expect("validated two-byte Thumb instruction"),
    ))
}

pub fn read_thumb_bl(rom: &[u8], address: u32) -> Result<(u16, u16), DecodeError> {
    let offset = address
        .checked_sub(ROM_BASE)
        .ok_or(DecodeError::OutOfRange(address))? as usize;
    if offset + 4 > rom.len() {
        return Err(DecodeError::Truncated(address));
    }
    Ok((
        u16::from_le_bytes(
            rom[offset..offset + 2]
                .try_into()
                .expect("validated first Thumb BL halfword"),
        ),
        u16::from_le_bytes(
            rom[offset + 2..offset + 4]
                .try_into()
                .expect("validated second Thumb BL halfword"),
        ),
    ))
}
