/// CPU core for the Game Boy emulator.
///
/// This struct defines the CPU registers and basic operations such as reset and step.
/// The implementation is intentionally minimal at this stage and will grow over time
/// to support the full Game Boy instruction set.
#[derive(Debug)]
pub struct Cpu {
    // 8-bit registers
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    // 16-bit registers
    pub sp: u16,
    pub pc: u16,
}

impl Cpu {
    /// Creates a new CPU with all registers set to zero.
    pub fn new() -> Self {
        Self {
            a: 0,
            f: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            h: 0,
            l: 0,
            sp: 0,
            pc: 0,
        }
    }

    /// Resets the CPU registers to the power-on state of the original Game Boy.
    pub fn reset(&mut self) {
        self.a = 0x01;
        self.f = 0xB0;
        self.b = 0x00;
        self.c = 0x13;
        self.d = 0x00;
        self.e = 0xD8;
        self.h = 0x01;
        self.l = 0x4D;
        self.sp = 0xFFFE;
        self.pc = 0x0100;
    }

    /// Executes a single CPU step (fetch/decode/execute cycle).
    /// Currently this is a stub and does nothing.
    pub fn step(&mut self, _memory: &mut [u8; 0x10000]) {
        // TODO: implement instruction decoding and execution
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_sets_initial_values() {
        let mut cpu = Cpu::new();
        cpu.reset();
        assert_eq!(cpu.a, 0x01);
        assert_eq!(cpu.f, 0xB0);
        assert_eq!(cpu.b, 0x00);
        assert_eq!(cpu.c, 0x13);
        assert_eq!(cpu.d, 0x00);
        assert_eq!(cpu.e, 0xD8);
        assert_eq!(cpu.h, 0x01);
        assert_eq!(cpu.l, 0x4D);
        assert_eq!(cpu.sp, 0xFFFE);
        assert_eq!(cpu.pc, 0x0100);
    }
}

