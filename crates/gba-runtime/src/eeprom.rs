#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EepromSize {
    Kbit4,
    Kbit64,
}

impl EepromSize {
    fn address_bits(self) -> usize {
        match self {
            Self::Kbit4 => 6,
            Self::Kbit64 => 14,
        }
    }

    fn bytes(self) -> usize {
        match self {
            Self::Kbit4 => 512,
            Self::Kbit64 => 8192,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Eeprom {
    size: EepromSize,
    data: Vec<u8>,
}

impl Eeprom {
    pub fn new(size: EepromSize) -> Self {
        Self {
            size,
            data: vec![0xff; size.bytes()],
        }
    }

    pub fn size(&self) -> EepromSize {
        self.size
    }

    pub fn read_byte(&self, address: usize) -> u8 {
        self.data[address % self.data.len()]
    }

    pub fn write_byte(&mut self, address: usize, value: u8) {
        let index = address % self.data.len();
        self.data[index] = value;
    }

    pub fn transfer(&mut self, input: &[bool]) -> Vec<bool> {
        let address_bits = self.size.address_bits();
        let write_len = 1 + 2 + address_bits + 64;
        let read_len = 1 + 2 + address_bits;

        if input.len() == write_len && input[0] && !input[1] && input[2] {
            let address = bits_to_usize(&input[3..3 + address_bits]);
            let data_start = 3 + address_bits;
            for byte in 0..8 {
                let start = data_start + byte * 8;
                let value = bits_to_u8(&input[start..start + 8]);
                self.write_byte(address * 8 + byte, value);
            }
            return Vec::new();
        }

        if input.len() == read_len && input[0] && input[1] && input[2] {
            let address = bits_to_usize(&input[3..3 + address_bits]);
            let mut output = Vec::with_capacity(4 + 64);
            output.extend([false, false, false, false]);
            for byte in 0..8 {
                let value = self.read_byte(address * 8 + byte);
                for bit in (0..8).rev() {
                    output.push((value & (1 << bit)) != 0);
                }
            }
            return output;
        }

        Vec::new()
    }
}

fn bits_to_usize(bits: &[bool]) -> usize {
    bits.iter()
        .fold(0usize, |value, &bit| (value << 1) | usize::from(bit))
}

fn bits_to_u8(bits: &[bool]) -> u8 {
    bits.iter()
        .fold(0u8, |value, &bit| (value << 1) | u8::from(bit))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(value: usize, width: usize) -> Vec<bool> {
        (0..width)
            .rev()
            .map(|bit| (value & (1 << bit)) != 0)
            .collect()
    }

    fn write_transaction(address: usize, data: [u8; 8], address_bits: usize) -> Vec<bool> {
        let mut input = vec![true, false, true];
        input.extend(bits(address, address_bits));
        for value in data {
            input.extend(bits(value as usize, 8));
        }
        input
    }

    fn read_transaction(address: usize, address_bits: usize) -> Vec<bool> {
        let mut input = vec![true, true, true];
        input.extend(bits(address, address_bits));
        input
    }

    #[test]
    fn eeprom_512b_write_then_read_round_trips_eight_bytes() {
        let mut eeprom = Eeprom::new(EepromSize::Kbit4);
        let data = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0];

        assert!(eeprom.transfer(&write_transaction(7, data, 6)).is_empty());
        let output = eeprom.transfer(&read_transaction(7, 6));

        assert_eq!(output.len(), 68);
        assert!(output[..4].iter().all(|&bit| !bit));
        let payload = output[4..]
            .as_chunks::<8>()
            .0
            .iter()
            .map(|chunk| bits_to_u8(chunk))
            .collect::<Vec<_>>();
        assert_eq!(payload, data);
    }

    #[test]
    fn eeprom_8kib_uses_fourteen_bit_addresses() {
        let mut eeprom = Eeprom::new(EepromSize::Kbit64);
        let data = [0xa5; 8];
        let address = 0x123;

        eeprom.transfer(&write_transaction(address, data, 14));
        let output = eeprom.transfer(&read_transaction(address, 14));
        let payload = output[4..]
            .as_chunks::<8>()
            .0
            .iter()
            .map(|chunk| bits_to_u8(chunk))
            .collect::<Vec<_>>();

        assert_eq!(payload, data);
    }

    #[test]
    fn invalid_serial_transaction_has_no_side_effect() {
        let mut eeprom = Eeprom::new(EepromSize::Kbit4);
        let before = eeprom.clone();

        assert!(eeprom.transfer(&[true, false]).is_empty());
        assert_eq!(eeprom, before);
    }
}
