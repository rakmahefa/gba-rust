use thiserror::Error;

pub const BIOS_SIZE: usize = 0x4000;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BiosLoadError {
    #[error("invalid BIOS size: expected {expected:#x} bytes, got {actual:#x}")]
    InvalidSize { expected: usize, actual: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bios {
    data: Box<[u8; BIOS_SIZE]>,
}

impl Bios {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BiosLoadError> {
        if bytes.len() != BIOS_SIZE {
            return Err(BiosLoadError::InvalidSize {
                expected: BIOS_SIZE,
                actual: bytes.len(),
            });
        }

        let mut data = Box::new([0; BIOS_SIZE]);
        data.copy_from_slice(bytes);
        Ok(Self { data })
    }

    pub fn zeroed() -> Self {
        Self {
            data: Box::new([0; BIOS_SIZE]),
        }
    }

    #[inline]
    pub fn read8(&self, offset: usize) -> u8 {
        self.data[offset]
    }

    #[inline]
    pub fn bytes(&self) -> &[u8; BIOS_SIZE] {
        &self.data
    }
}

impl Default for Bios {
    fn default() -> Self {
        Self::zeroed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_exact_gba_bios_size() {
        let bytes = vec![0x5a; BIOS_SIZE];
        let bios = Bios::from_bytes(&bytes).unwrap();
        assert_eq!(bios.read8(0), 0x5a);
        assert_eq!(bios.read8(BIOS_SIZE - 1), 0x5a);
    }

    #[test]
    fn rejects_truncated_bios() {
        let error = Bios::from_bytes(&[0; BIOS_SIZE - 1]).unwrap_err();
        assert_eq!(
            error,
            BiosLoadError::InvalidSize {
                expected: BIOS_SIZE,
                actual: BIOS_SIZE - 1,
            }
        );
    }

    #[test]
    fn rejects_oversized_bios() {
        let error = Bios::from_bytes(&vec![0; BIOS_SIZE + 1]).unwrap_err();
        assert_eq!(
            error,
            BiosLoadError::InvalidSize {
                expected: BIOS_SIZE,
                actual: BIOS_SIZE + 1,
            }
        );
    }
}
