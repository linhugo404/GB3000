/// Memory subsystem for the Game Boy emulator.
///
/// This simple representation stores the entire 64KB address space in a fixed
/// array. More sophisticated banking and memory-mapped I/O will be added later.
#[derive(Debug)]
pub struct Memory {
    pub data: [u8; 0x10000],
}

impl Memory {
    /// Creates new memory initialized to zero.
    pub fn new() -> Self {
        Self { data: [0; 0x10000] }
    }

    /// Reads a byte from the given address.
    pub fn read_byte(&self, addr: u16) -> u8 {
        self.data[addr as usize]
    }

    /// Writes a byte to the given address.
    pub fn write_byte(&mut self, addr: u16, value: u8) {
        self.data[addr as usize] = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write_roundtrip() {
        let mut mem = Memory::new();
        mem.write_byte(0xC000, 0x42);
        assert_eq!(mem.read_byte(0xC000), 0x42);
    }
}

