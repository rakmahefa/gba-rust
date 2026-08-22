use thiserror::Error;

pub const ROM_BASE: u32 = 0x0800_0000;

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("address {0:#x} is outside the mapped image")]
    OutOfRange(u32),
    #[error("truncated instruction at {0:#x}")]
    Truncated(u32),
}

fn offset_for(rom: &[u8], base: u32, address: u32, width: usize) -> Result<usize, DecodeError> {
    let offset = address
        .checked_sub(base)
        .ok_or(DecodeError::OutOfRange(address))? as usize;
    if offset.checked_add(width).is_none_or(|end| end > rom.len()) {
        return Err(DecodeError::Truncated(address));
    }
    Ok(offset)
}

pub fn read_arm_at(rom: &[u8], base: u32, address: u32) -> Result<u32, DecodeError> {
    let offset = offset_for(rom, base, address, 4)?;
    Ok(u32::from_le_bytes(
        rom[offset..offset + 4]
            .try_into()
            .expect("validated four-byte ARM instruction"),
    ))
}

pub fn read_thumb_at(rom: &[u8], base: u32, address: u32) -> Result<u16, DecodeError> {
    let offset = offset_for(rom, base, address, 2)?;
    Ok(u16::from_le_bytes(
        rom[offset..offset + 2]
            .try_into()
            .expect("validated two-byte Thumb instruction"),
    ))
}

pub fn read_thumb_bl_at(rom: &[u8], base: u32, address: u32) -> Result<(u16, u16), DecodeError> {
    let offset = offset_for(rom, base, address, 4)?;
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

pub fn read_arm(rom: &[u8], address: u32) -> Result<u32, DecodeError> {
    read_arm_at(rom, ROM_BASE, address)
}

pub fn read_thumb(rom: &[u8], address: u32) -> Result<u16, DecodeError> {
    read_thumb_at(rom, ROM_BASE, address)
}

pub fn read_thumb_bl(rom: &[u8], address: u32) -> Result<(u16, u16), DecodeError> {
    read_thumb_bl_at(rom, ROM_BASE, address)
}
