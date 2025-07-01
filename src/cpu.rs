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
    ///
    /// Only a very small subset of instructions are currently supported. This
    /// will be expanded over time as the emulator grows.
    pub fn step(&mut self, memory: &mut [u8; 0x10000]) {
        let opcode = memory[self.pc as usize];
        self.pc = self.pc.wrapping_add(1);

        match opcode {
            // 0x00: NOP
            0x00 => {}

            // 0x06: LD B, d8
            0x06 => {
                let val = memory[self.pc as usize];
                self.pc = self.pc.wrapping_add(1);
                self.b = val;
            }

            // 0x0E: LD C, d8
            0x0E => {
                let val = memory[self.pc as usize];
                self.pc = self.pc.wrapping_add(1);
                self.c = val;
            }

            // 0x16: LD D, d8
            0x16 => {
                let val = memory[self.pc as usize];
                self.pc = self.pc.wrapping_add(1);
                self.d = val;
            }

            // 0x1E: LD E, d8
            0x1E => {
                let val = memory[self.pc as usize];
                self.pc = self.pc.wrapping_add(1);
                self.e = val;
            }


            // 0x3E: LD A, d8
            0x3E => {
                let val = memory[self.pc as usize];
                self.pc = self.pc.wrapping_add(1);
                self.a = val;
            }

            // 0xAF: XOR A
            0xAF => {
                self.a ^= self.a;
                self.f = 0;
            }

            // 0x0C: INC C
            0x0C => {
                self.c = self.c.wrapping_add(1);
            }

            // 0x0D: DEC C
            0x0D => {
                self.c = self.c.wrapping_sub(1);
            }

            // 0x14: INC D
            0x14 => {
                self.d = self.d.wrapping_add(1);
            }

            // 0x15: DEC D
            0x15 => {
                self.d = self.d.wrapping_sub(1);
            }

            // 0x1C: INC E
            0x1C => {
                self.e = self.e.wrapping_add(1);
            }

            // 0x1D: DEC E
            0x1D => {
                self.e = self.e.wrapping_sub(1);
            }

            // 0x24: INC H
            0x24 => {
                self.h = self.h.wrapping_add(1);
            }

            // 0x25: DEC H
            0x25 => {
                self.h = self.h.wrapping_sub(1);
            }

            // 0x2C: INC L
            0x2C => {
                self.l = self.l.wrapping_add(1);
            }

            // 0x2D: DEC L
            0x2D => {
                self.l = self.l.wrapping_sub(1);
            }

            // 0x3C: INC A
            0x3C => {
                self.a = self.a.wrapping_add(1);
            }

            // 0x3D: DEC A
            0x3D => {
                self.a = self.a.wrapping_sub(1);
            }

            // 0x04: INC B
            0x04 => {
                self.b = self.b.wrapping_add(1);
            }

            // 0x05: DEC B
            0x05 => {
                self.b = self.b.wrapping_sub(1);
            }

            op => panic!("Unimplemented opcode {op:02X}"),
        }
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

    #[test]
    fn step_executes_nop() {
        let mut cpu = Cpu::new();
        let mut mem = [0u8; 0x10000];
        mem[0x0100] = 0x00; // NOP
        cpu.reset();
        cpu.step(&mut mem);
        assert_eq!(cpu.pc, 0x0101);
    }

    #[test]
    fn step_executes_ld_b_d8() {
        let mut cpu = Cpu::new();
        let mut mem = [0u8; 0x10000];
        mem[0x0100] = 0x06; // LD B,d8
        mem[0x0101] = 0x42;
        cpu.reset();
        cpu.step(&mut mem);
        assert_eq!(cpu.b, 0x42);
        assert_eq!(cpu.pc, 0x0102);
    }

    #[test]
    fn step_executes_ld_d_d8() {
        let mut cpu = Cpu::new();
        let mut mem = [0u8; 0x10000];
        mem[0x0100] = 0x16; // LD D,d8
        mem[0x0101] = 0x99;
        cpu.reset();
        cpu.step(&mut mem);
        assert_eq!(cpu.d, 0x99);
        assert_eq!(cpu.pc, 0x0102);
    }

    #[test]
    fn step_executes_inc_c() {
        let mut cpu = Cpu::new();
        let mut mem = [0u8; 0x10000];
        mem[0x0100] = 0x0C; // INC C
        cpu.reset();
        cpu.c = 0x01;
        cpu.step(&mut mem);
        assert_eq!(cpu.c, 0x02);
        assert_eq!(cpu.pc, 0x0101);
    }
}

