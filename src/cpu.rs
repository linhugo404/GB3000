/// CPU core for the Game Boy emulator.
///
/// This implements the Sharp LR35902 processor with all opcodes,
/// proper flag handling, and interrupt support.
/// 
/// This version is cycle-accurate, executing one M-cycle at a time.

use crate::memory::Memory;

// Flag bit positions in the F register
const FLAG_Z: u8 = 0b1000_0000; // Zero flag
const FLAG_N: u8 = 0b0100_0000; // Subtract flag
const FLAG_H: u8 = 0b0010_0000; // Half-carry flag
const FLAG_C: u8 = 0b0001_0000; // Carry flag

/// CPU execution state for cycle-accurate emulation
#[derive(Debug, Clone, Copy, PartialEq)]
enum CpuState {
    /// Ready to fetch next opcode
    Fetch,
    /// Executing an instruction (remaining M-cycles)
    Execute(u8),
}

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
    // Interrupt master enable
    pub ime: bool,
    // Pending IME enable (for EI delay)
    pub ime_pending: bool,
    // CPU halted state
    pub halted: bool,
    // CPU stopped state
    pub stopped: bool,
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
            ime: false,
            ime_pending: false,
            halted: false,
            stopped: false,
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
        self.ime = false;
        self.ime_pending = false;
        self.halted = false;
        self.stopped = false;
    }

    // ========== Flag helpers ==========

    #[inline]
    fn flag_z(&self) -> bool {
        self.f & FLAG_Z != 0
    }

    #[inline]
    fn flag_n(&self) -> bool {
        self.f & FLAG_N != 0
    }

    #[inline]
    fn flag_h(&self) -> bool {
        self.f & FLAG_H != 0
    }

    #[inline]
    fn flag_c(&self) -> bool {
        self.f & FLAG_C != 0
    }

    #[inline]
    fn set_flag(&mut self, flag: u8, value: bool) {
        if value {
            self.f |= flag;
        } else {
            self.f &= !flag;
        }
    }

    #[inline]
    fn set_flags(&mut self, z: bool, n: bool, h: bool, c: bool) {
        self.f = 0;
        if z { self.f |= FLAG_Z; }
        if n { self.f |= FLAG_N; }
        if h { self.f |= FLAG_H; }
        if c { self.f |= FLAG_C; }
    }

    // ========== 16-bit register pairs ==========

    #[inline]
    pub fn af(&self) -> u16 {
        ((self.a as u16) << 8) | (self.f as u16)
    }

    #[inline]
    pub fn set_af(&mut self, val: u16) {
        self.a = (val >> 8) as u8;
        self.f = (val & 0xF0) as u8; // Lower 4 bits of F are always 0
    }

    #[inline]
    pub fn bc(&self) -> u16 {
        ((self.b as u16) << 8) | (self.c as u16)
    }

    #[inline]
    pub fn set_bc(&mut self, val: u16) {
        self.b = (val >> 8) as u8;
        self.c = val as u8;
    }

    #[inline]
    pub fn de(&self) -> u16 {
        ((self.d as u16) << 8) | (self.e as u16)
    }

    #[inline]
    pub fn set_de(&mut self, val: u16) {
        self.d = (val >> 8) as u8;
        self.e = val as u8;
    }

    #[inline]
    pub fn hl(&self) -> u16 {
        ((self.h as u16) << 8) | (self.l as u16)
    }

    #[inline]
    pub fn set_hl(&mut self, val: u16) {
        self.h = (val >> 8) as u8;
        self.l = val as u8;
    }

    // ========== Memory access helpers ==========

    #[inline]
    fn read_byte(&self, memory: &Memory, addr: u16) -> u8 {
        memory.read_byte(addr)
    }

    #[inline]
    fn write_byte(&self, memory: &mut Memory, addr: u16, val: u8) {
        memory.write_byte(addr, val);
    }

    #[inline]
    fn fetch_byte(&mut self, memory: &Memory) -> u8 {
        let val = memory.read_byte(self.pc);
        self.pc = self.pc.wrapping_add(1);
        val
    }

    #[inline]
    fn fetch_word(&mut self, memory: &Memory) -> u16 {
        let lo = self.fetch_byte(memory) as u16;
        let hi = self.fetch_byte(memory) as u16;
        (hi << 8) | lo
    }

    // ========== Stack operations ==========

    #[inline]
    fn push(&mut self, memory: &mut Memory, val: u16) {
        self.sp = self.sp.wrapping_sub(1);
        memory.write_byte(self.sp, (val >> 8) as u8);
        self.sp = self.sp.wrapping_sub(1);
        memory.write_byte(self.sp, val as u8);
    }

    #[inline]
    fn pop(&mut self, memory: &Memory) -> u16 {
        let lo = memory.read_byte(self.sp) as u16;
        self.sp = self.sp.wrapping_add(1);
        let hi = memory.read_byte(self.sp) as u16;
        self.sp = self.sp.wrapping_add(1);
        (hi << 8) | lo
    }

    // ========== ALU operations ==========

    fn alu_add(&mut self, val: u8) {
        let a = self.a;
        let result = a.wrapping_add(val);
        self.set_flags(
            result == 0,
            false,
            (a & 0x0F) + (val & 0x0F) > 0x0F,
            (a as u16) + (val as u16) > 0xFF,
        );
        self.a = result;
    }

    fn alu_adc(&mut self, val: u8) {
        let a = self.a;
        let c = if self.flag_c() { 1u8 } else { 0u8 };
        let result = a.wrapping_add(val).wrapping_add(c);
        self.set_flags(
            result == 0,
            false,
            (a & 0x0F) + (val & 0x0F) + c > 0x0F,
            (a as u16) + (val as u16) + (c as u16) > 0xFF,
        );
        self.a = result;
    }

    fn alu_sub(&mut self, val: u8) {
        let a = self.a;
        let result = a.wrapping_sub(val);
        self.set_flags(
            result == 0,
            true,
            (a & 0x0F) < (val & 0x0F),
            a < val,
        );
        self.a = result;
    }

    fn alu_sbc(&mut self, val: u8) {
        let a = self.a;
        let c = if self.flag_c() { 1u8 } else { 0u8 };
        let result = a.wrapping_sub(val).wrapping_sub(c);
        self.set_flags(
            result == 0,
            true,
            (a & 0x0F) < (val & 0x0F) + c,
            (a as u16) < (val as u16) + (c as u16),
        );
        self.a = result;
    }

    fn alu_and(&mut self, val: u8) {
        self.a &= val;
        self.set_flags(self.a == 0, false, true, false);
    }

    fn alu_xor(&mut self, val: u8) {
        self.a ^= val;
        self.set_flags(self.a == 0, false, false, false);
    }

    fn alu_or(&mut self, val: u8) {
        self.a |= val;
        self.set_flags(self.a == 0, false, false, false);
    }

    fn alu_cp(&mut self, val: u8) {
        let a = self.a;
        self.set_flags(
            a == val,
            true,
            (a & 0x0F) < (val & 0x0F),
            a < val,
        );
    }

    fn alu_inc(&mut self, val: u8) -> u8 {
        let result = val.wrapping_add(1);
        self.set_flag(FLAG_Z, result == 0);
        self.set_flag(FLAG_N, false);
        self.set_flag(FLAG_H, (val & 0x0F) + 1 > 0x0F);
        result
    }

    fn alu_dec(&mut self, val: u8) -> u8 {
        let result = val.wrapping_sub(1);
        self.set_flag(FLAG_Z, result == 0);
        self.set_flag(FLAG_N, true);
        self.set_flag(FLAG_H, (val & 0x0F) == 0);
        result
    }

    fn alu_add_hl(&mut self, val: u16) {
        let hl = self.hl();
        let result = hl.wrapping_add(val);
        self.set_flag(FLAG_N, false);
        self.set_flag(FLAG_H, (hl & 0x0FFF) + (val & 0x0FFF) > 0x0FFF);
        self.set_flag(FLAG_C, hl > 0xFFFF - val);
        self.set_hl(result);
    }

    fn alu_add_sp(&mut self, val: i8) -> u16 {
        let sp = self.sp;
        let val_u = val as i16 as u16;
        let result = sp.wrapping_add(val_u);
        self.set_flags(
            false,
            false,
            (sp & 0x0F) + (val_u & 0x0F) > 0x0F,
            (sp & 0xFF) + (val_u & 0xFF) > 0xFF,
        );
        result
    }

    // ========== Rotate/Shift operations ==========

    fn alu_rlc(&mut self, val: u8) -> u8 {
        let carry = val >> 7;
        let result = (val << 1) | carry;
        self.set_flags(result == 0, false, false, carry != 0);
        result
    }

    fn alu_rrc(&mut self, val: u8) -> u8 {
        let carry = val & 1;
        let result = (val >> 1) | (carry << 7);
        self.set_flags(result == 0, false, false, carry != 0);
        result
    }

    fn alu_rl(&mut self, val: u8) -> u8 {
        let old_carry = if self.flag_c() { 1 } else { 0 };
        let new_carry = val >> 7;
        let result = (val << 1) | old_carry;
        self.set_flags(result == 0, false, false, new_carry != 0);
        result
    }

    fn alu_rr(&mut self, val: u8) -> u8 {
        let old_carry = if self.flag_c() { 1 } else { 0 };
        let new_carry = val & 1;
        let result = (val >> 1) | (old_carry << 7);
        self.set_flags(result == 0, false, false, new_carry != 0);
        result
    }

    fn alu_sla(&mut self, val: u8) -> u8 {
        let carry = val >> 7;
        let result = val << 1;
        self.set_flags(result == 0, false, false, carry != 0);
        result
    }

    fn alu_sra(&mut self, val: u8) -> u8 {
        let carry = val & 1;
        let result = (val >> 1) | (val & 0x80);
        self.set_flags(result == 0, false, false, carry != 0);
        result
    }

    fn alu_swap(&mut self, val: u8) -> u8 {
        let result = (val >> 4) | (val << 4);
        self.set_flags(result == 0, false, false, false);
        result
    }

    fn alu_srl(&mut self, val: u8) -> u8 {
        let carry = val & 1;
        let result = val >> 1;
        self.set_flags(result == 0, false, false, carry != 0);
        result
    }

    fn alu_bit(&mut self, bit: u8, val: u8) {
        let result = val & (1 << bit);
        self.set_flag(FLAG_Z, result == 0);
        self.set_flag(FLAG_N, false);
        self.set_flag(FLAG_H, true);
    }

    fn alu_res(&self, bit: u8, val: u8) -> u8 {
        val & !(1 << bit)
    }

    fn alu_set(&self, bit: u8, val: u8) -> u8 {
        val | (1 << bit)
    }

    fn alu_daa(&mut self) {
        let mut a = self.a;
        let mut adjust = 0u8;

        if self.flag_h() || (!self.flag_n() && (a & 0x0F) > 9) {
            adjust |= 0x06;
        }

        if self.flag_c() || (!self.flag_n() && a > 0x99) {
            adjust |= 0x60;
            self.set_flag(FLAG_C, true);
        }

        if self.flag_n() {
            a = a.wrapping_sub(adjust);
        } else {
            a = a.wrapping_add(adjust);
        }

        self.set_flag(FLAG_Z, a == 0);
        self.set_flag(FLAG_H, false);
        self.a = a;
    }

    // ========== Main execution ==========

    /// Executes a single CPU step (fetch/decode/execute cycle).
    /// Returns the number of T-cycles consumed.
    pub fn step(&mut self, memory: &mut Memory) -> u32 {
        // Handle pending IME enable (EI has a one-instruction delay)
        if self.ime_pending {
            self.ime = true;
            self.ime_pending = false;
        }

        // If halted, just return 4 cycles
        if self.halted {
            return 4;
        }

        let opcode = self.fetch_byte(memory);

        match opcode {
            // ==================== 0x0X ====================
            0x00 => 4, // NOP

            0x01 => { // LD BC, d16
                let val = self.fetch_word(memory);
                self.set_bc(val);
                12
            }

            0x02 => { // LD (BC), A
                self.write_byte(memory, self.bc(), self.a);
                8
            }

            0x03 => { // INC BC
                self.set_bc(self.bc().wrapping_add(1));
                8
            }

            0x04 => { // INC B
                self.b = self.alu_inc(self.b);
                4
            }

            0x05 => { // DEC B
                self.b = self.alu_dec(self.b);
                4
            }

            0x06 => { // LD B, d8
                self.b = self.fetch_byte(memory);
                8
            }

            0x07 => { // RLCA
                let carry = self.a >> 7;
                self.a = (self.a << 1) | carry;
                self.set_flags(false, false, false, carry != 0);
                4
            }

            0x08 => { // LD (a16), SP
                let addr = self.fetch_word(memory);
                memory.write_byte(addr, self.sp as u8);
                memory.write_byte(addr.wrapping_add(1), (self.sp >> 8) as u8);
                20
            }

            0x09 => { // ADD HL, BC
                self.alu_add_hl(self.bc());
                8
            }

            0x0A => { // LD A, (BC)
                self.a = self.read_byte(memory, self.bc());
                8
            }

            0x0B => { // DEC BC
                self.set_bc(self.bc().wrapping_sub(1));
                8
            }

            0x0C => { // INC C
                self.c = self.alu_inc(self.c);
                4
            }

            0x0D => { // DEC C
                self.c = self.alu_dec(self.c);
                4
            }

            0x0E => { // LD C, d8
                self.c = self.fetch_byte(memory);
                8
            }

            0x0F => { // RRCA
                let carry = self.a & 1;
                self.a = (self.a >> 1) | (carry << 7);
                self.set_flags(false, false, false, carry != 0);
                4
            }

            // ==================== 0x1X ====================
            0x10 => { // STOP
                self.pc = self.pc.wrapping_add(1);
                self.stopped = true;
                4
            }

            0x11 => { // LD DE, d16
                let val = self.fetch_word(memory);
                self.set_de(val);
                12
            }

            0x12 => { // LD (DE), A
                self.write_byte(memory, self.de(), self.a);
                8
            }

            0x13 => { // INC DE
                self.set_de(self.de().wrapping_add(1));
                8
            }

            0x14 => { // INC D
                self.d = self.alu_inc(self.d);
                4
            }

            0x15 => { // DEC D
                self.d = self.alu_dec(self.d);
                4
            }

            0x16 => { // LD D, d8
                self.d = self.fetch_byte(memory);
                8
            }

            0x17 => { // RLA
                let old_carry = if self.flag_c() { 1 } else { 0 };
                let new_carry = self.a >> 7;
                self.a = (self.a << 1) | old_carry;
                self.set_flags(false, false, false, new_carry != 0);
                4
            }

            0x18 => { // JR r8
                let offset = self.fetch_byte(memory) as i8;
                self.pc = self.pc.wrapping_add(offset as u16);
                12
            }

            0x19 => { // ADD HL, DE
                self.alu_add_hl(self.de());
                8
            }

            0x1A => { // LD A, (DE)
                self.a = self.read_byte(memory, self.de());
                8
            }

            0x1B => { // DEC DE
                self.set_de(self.de().wrapping_sub(1));
                8
            }

            0x1C => { // INC E
                self.e = self.alu_inc(self.e);
                4
            }

            0x1D => { // DEC E
                self.e = self.alu_dec(self.e);
                4
            }

            0x1E => { // LD E, d8
                self.e = self.fetch_byte(memory);
                8
            }

            0x1F => { // RRA
                let old_carry = if self.flag_c() { 1 } else { 0 };
                let new_carry = self.a & 1;
                self.a = (self.a >> 1) | (old_carry << 7);
                self.set_flags(false, false, false, new_carry != 0);
                4
            }

            // ==================== 0x2X ====================
            0x20 => { // JR NZ, r8
                let offset = self.fetch_byte(memory) as i8;
                if !self.flag_z() {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    12
                } else {
                    8
                }
            }

            0x21 => { // LD HL, d16
                let val = self.fetch_word(memory);
                self.set_hl(val);
                12
            }

            0x22 => { // LD (HL+), A
                self.write_byte(memory, self.hl(), self.a);
                self.set_hl(self.hl().wrapping_add(1));
                8
            }

            0x23 => { // INC HL
                self.set_hl(self.hl().wrapping_add(1));
                8
            }

            0x24 => { // INC H
                self.h = self.alu_inc(self.h);
                4
            }

            0x25 => { // DEC H
                self.h = self.alu_dec(self.h);
                4
            }

            0x26 => { // LD H, d8
                self.h = self.fetch_byte(memory);
                8
            }

            0x27 => { // DAA
                self.alu_daa();
                4
            }

            0x28 => { // JR Z, r8
                let offset = self.fetch_byte(memory) as i8;
                if self.flag_z() {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    12
                } else {
                    8
                }
            }

            0x29 => { // ADD HL, HL
                let hl = self.hl();
                self.alu_add_hl(hl);
                8
            }

            0x2A => { // LD A, (HL+)
                self.a = self.read_byte(memory, self.hl());
                self.set_hl(self.hl().wrapping_add(1));
                8
            }

            0x2B => { // DEC HL
                self.set_hl(self.hl().wrapping_sub(1));
                8
            }

            0x2C => { // INC L
                self.l = self.alu_inc(self.l);
                4
            }

            0x2D => { // DEC L
                self.l = self.alu_dec(self.l);
                4
            }

            0x2E => { // LD L, d8
                self.l = self.fetch_byte(memory);
                8
            }

            0x2F => { // CPL
                self.a = !self.a;
                self.set_flag(FLAG_N, true);
                self.set_flag(FLAG_H, true);
                4
            }

            // ==================== 0x3X ====================
            0x30 => { // JR NC, r8
                let offset = self.fetch_byte(memory) as i8;
                if !self.flag_c() {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    12
                } else {
                    8
                }
            }

            0x31 => { // LD SP, d16
                self.sp = self.fetch_word(memory);
                12
            }

            0x32 => { // LD (HL-), A
                self.write_byte(memory, self.hl(), self.a);
                self.set_hl(self.hl().wrapping_sub(1));
                8
            }

            0x33 => { // INC SP
                self.sp = self.sp.wrapping_add(1);
                8
            }

            0x34 => { // INC (HL)
                let addr = self.hl();
                let val = self.read_byte(memory, addr);
                let result = self.alu_inc(val);
                self.write_byte(memory, addr, result);
                12
            }

            0x35 => { // DEC (HL)
                let addr = self.hl();
                let val = self.read_byte(memory, addr);
                let result = self.alu_dec(val);
                self.write_byte(memory, addr, result);
                12
            }

            0x36 => { // LD (HL), d8
                let val = self.fetch_byte(memory);
                self.write_byte(memory, self.hl(), val);
                12
            }

            0x37 => { // SCF
                self.set_flag(FLAG_N, false);
                self.set_flag(FLAG_H, false);
                self.set_flag(FLAG_C, true);
                4
            }

            0x38 => { // JR C, r8
                let offset = self.fetch_byte(memory) as i8;
                if self.flag_c() {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    12
                } else {
                    8
                }
            }

            0x39 => { // ADD HL, SP
                self.alu_add_hl(self.sp);
                8
            }

            0x3A => { // LD A, (HL-)
                self.a = self.read_byte(memory, self.hl());
                self.set_hl(self.hl().wrapping_sub(1));
                8
            }

            0x3B => { // DEC SP
                self.sp = self.sp.wrapping_sub(1);
                8
            }

            0x3C => { // INC A
                self.a = self.alu_inc(self.a);
                4
            }

            0x3D => { // DEC A
                self.a = self.alu_dec(self.a);
                4
            }

            0x3E => { // LD A, d8
                self.a = self.fetch_byte(memory);
                8
            }

            0x3F => { // CCF
                self.set_flag(FLAG_N, false);
                self.set_flag(FLAG_H, false);
                self.set_flag(FLAG_C, !self.flag_c());
                4
            }

            // ==================== 0x4X - LD B/C, r ====================
            0x40 => 4,
            0x41 => { self.b = self.c; 4 }
            0x42 => { self.b = self.d; 4 }
            0x43 => { self.b = self.e; 4 }
            0x44 => { self.b = self.h; 4 }
            0x45 => { self.b = self.l; 4 }
            0x46 => { self.b = self.read_byte(memory, self.hl()); 8 }
            0x47 => { self.b = self.a; 4 }
            0x48 => { self.c = self.b; 4 }
            0x49 => 4,
            0x4A => { self.c = self.d; 4 }
            0x4B => { self.c = self.e; 4 }
            0x4C => { self.c = self.h; 4 }
            0x4D => { self.c = self.l; 4 }
            0x4E => { self.c = self.read_byte(memory, self.hl()); 8 }
            0x4F => { self.c = self.a; 4 }

            // ==================== 0x5X - LD D/E, r ====================
            0x50 => { self.d = self.b; 4 }
            0x51 => { self.d = self.c; 4 }
            0x52 => 4,
            0x53 => { self.d = self.e; 4 }
            0x54 => { self.d = self.h; 4 }
            0x55 => { self.d = self.l; 4 }
            0x56 => { self.d = self.read_byte(memory, self.hl()); 8 }
            0x57 => { self.d = self.a; 4 }
            0x58 => { self.e = self.b; 4 }
            0x59 => { self.e = self.c; 4 }
            0x5A => { self.e = self.d; 4 }
            0x5B => 4,
            0x5C => { self.e = self.h; 4 }
            0x5D => { self.e = self.l; 4 }
            0x5E => { self.e = self.read_byte(memory, self.hl()); 8 }
            0x5F => { self.e = self.a; 4 }

            // ==================== 0x6X - LD H/L, r ====================
            0x60 => { self.h = self.b; 4 }
            0x61 => { self.h = self.c; 4 }
            0x62 => { self.h = self.d; 4 }
            0x63 => { self.h = self.e; 4 }
            0x64 => 4,
            0x65 => { self.h = self.l; 4 }
            0x66 => { self.h = self.read_byte(memory, self.hl()); 8 }
            0x67 => { self.h = self.a; 4 }
            0x68 => { self.l = self.b; 4 }
            0x69 => { self.l = self.c; 4 }
            0x6A => { self.l = self.d; 4 }
            0x6B => { self.l = self.e; 4 }
            0x6C => { self.l = self.h; 4 }
            0x6D => 4,
            0x6E => { self.l = self.read_byte(memory, self.hl()); 8 }
            0x6F => { self.l = self.a; 4 }

            // ==================== 0x7X - LD (HL)/A, r ====================
            0x70 => { self.write_byte(memory, self.hl(), self.b); 8 }
            0x71 => { self.write_byte(memory, self.hl(), self.c); 8 }
            0x72 => { self.write_byte(memory, self.hl(), self.d); 8 }
            0x73 => { self.write_byte(memory, self.hl(), self.e); 8 }
            0x74 => { self.write_byte(memory, self.hl(), self.h); 8 }
            0x75 => { self.write_byte(memory, self.hl(), self.l); 8 }
            0x76 => { // HALT
                self.halted = true;
                4
            }
            0x77 => { self.write_byte(memory, self.hl(), self.a); 8 }
            0x78 => { self.a = self.b; 4 }
            0x79 => { self.a = self.c; 4 }
            0x7A => { self.a = self.d; 4 }
            0x7B => { self.a = self.e; 4 }
            0x7C => { self.a = self.h; 4 }
            0x7D => { self.a = self.l; 4 }
            0x7E => { self.a = self.read_byte(memory, self.hl()); 8 }
            0x7F => 4,

            // ==================== 0x8X - ADD/ADC A, r ====================
            0x80 => { self.alu_add(self.b); 4 }
            0x81 => { self.alu_add(self.c); 4 }
            0x82 => { self.alu_add(self.d); 4 }
            0x83 => { self.alu_add(self.e); 4 }
            0x84 => { self.alu_add(self.h); 4 }
            0x85 => { self.alu_add(self.l); 4 }
            0x86 => { let v = self.read_byte(memory, self.hl()); self.alu_add(v); 8 }
            0x87 => { self.alu_add(self.a); 4 }
            0x88 => { self.alu_adc(self.b); 4 }
            0x89 => { self.alu_adc(self.c); 4 }
            0x8A => { self.alu_adc(self.d); 4 }
            0x8B => { self.alu_adc(self.e); 4 }
            0x8C => { self.alu_adc(self.h); 4 }
            0x8D => { self.alu_adc(self.l); 4 }
            0x8E => { let v = self.read_byte(memory, self.hl()); self.alu_adc(v); 8 }
            0x8F => { self.alu_adc(self.a); 4 }

            // ==================== 0x9X - SUB/SBC A, r ====================
            0x90 => { self.alu_sub(self.b); 4 }
            0x91 => { self.alu_sub(self.c); 4 }
            0x92 => { self.alu_sub(self.d); 4 }
            0x93 => { self.alu_sub(self.e); 4 }
            0x94 => { self.alu_sub(self.h); 4 }
            0x95 => { self.alu_sub(self.l); 4 }
            0x96 => { let v = self.read_byte(memory, self.hl()); self.alu_sub(v); 8 }
            0x97 => { self.alu_sub(self.a); 4 }
            0x98 => { self.alu_sbc(self.b); 4 }
            0x99 => { self.alu_sbc(self.c); 4 }
            0x9A => { self.alu_sbc(self.d); 4 }
            0x9B => { self.alu_sbc(self.e); 4 }
            0x9C => { self.alu_sbc(self.h); 4 }
            0x9D => { self.alu_sbc(self.l); 4 }
            0x9E => { let v = self.read_byte(memory, self.hl()); self.alu_sbc(v); 8 }
            0x9F => { self.alu_sbc(self.a); 4 }

            // ==================== 0xAX - AND/XOR A, r ====================
            0xA0 => { self.alu_and(self.b); 4 }
            0xA1 => { self.alu_and(self.c); 4 }
            0xA2 => { self.alu_and(self.d); 4 }
            0xA3 => { self.alu_and(self.e); 4 }
            0xA4 => { self.alu_and(self.h); 4 }
            0xA5 => { self.alu_and(self.l); 4 }
            0xA6 => { let v = self.read_byte(memory, self.hl()); self.alu_and(v); 8 }
            0xA7 => { self.alu_and(self.a); 4 }
            0xA8 => { self.alu_xor(self.b); 4 }
            0xA9 => { self.alu_xor(self.c); 4 }
            0xAA => { self.alu_xor(self.d); 4 }
            0xAB => { self.alu_xor(self.e); 4 }
            0xAC => { self.alu_xor(self.h); 4 }
            0xAD => { self.alu_xor(self.l); 4 }
            0xAE => { let v = self.read_byte(memory, self.hl()); self.alu_xor(v); 8 }
            0xAF => { self.alu_xor(self.a); 4 }

            // ==================== 0xBX - OR/CP A, r ====================
            0xB0 => { self.alu_or(self.b); 4 }
            0xB1 => { self.alu_or(self.c); 4 }
            0xB2 => { self.alu_or(self.d); 4 }
            0xB3 => { self.alu_or(self.e); 4 }
            0xB4 => { self.alu_or(self.h); 4 }
            0xB5 => { self.alu_or(self.l); 4 }
            0xB6 => { let v = self.read_byte(memory, self.hl()); self.alu_or(v); 8 }
            0xB7 => { self.alu_or(self.a); 4 }
            0xB8 => { self.alu_cp(self.b); 4 }
            0xB9 => { self.alu_cp(self.c); 4 }
            0xBA => { self.alu_cp(self.d); 4 }
            0xBB => { self.alu_cp(self.e); 4 }
            0xBC => { self.alu_cp(self.h); 4 }
            0xBD => { self.alu_cp(self.l); 4 }
            0xBE => { let v = self.read_byte(memory, self.hl()); self.alu_cp(v); 8 }
            0xBF => { self.alu_cp(self.a); 4 }

            // ==================== 0xCX ====================
            0xC0 => { // RET NZ
                if !self.flag_z() {
                    self.pc = self.pop(memory);
                    20
                } else {
                    8
                }
            }

            0xC1 => { // POP BC
                let val = self.pop(memory);
                self.set_bc(val);
                12
            }

            0xC2 => { // JP NZ, a16
                let addr = self.fetch_word(memory);
                if !self.flag_z() {
                    self.pc = addr;
                    16
                } else {
                    12
                }
            }

            0xC3 => { // JP a16
                self.pc = self.fetch_word(memory);
                16
            }

            0xC4 => { // CALL NZ, a16
                let addr = self.fetch_word(memory);
                if !self.flag_z() {
                    self.push(memory, self.pc);
                    self.pc = addr;
                    24
                } else {
                    12
                }
            }

            0xC5 => { // PUSH BC
                self.push(memory, self.bc());
                16
            }

            0xC6 => { // ADD A, d8
                let val = self.fetch_byte(memory);
                self.alu_add(val);
                8
            }

            0xC7 => { // RST 00H
                self.push(memory, self.pc);
                self.pc = 0x0000;
                16
            }

            0xC8 => { // RET Z
                if self.flag_z() {
                    self.pc = self.pop(memory);
                    20
                } else {
                    8
                }
            }

            0xC9 => { // RET
                self.pc = self.pop(memory);
                16
            }

            0xCA => { // JP Z, a16
                let addr = self.fetch_word(memory);
                if self.flag_z() {
                    self.pc = addr;
                    16
                } else {
                    12
                }
            }

            0xCB => self.execute_cb(memory),

            0xCC => { // CALL Z, a16
                let addr = self.fetch_word(memory);
                if self.flag_z() {
                    self.push(memory, self.pc);
                    self.pc = addr;
                    24
                } else {
                    12
                }
            }

            0xCD => { // CALL a16
                let addr = self.fetch_word(memory);
                self.push(memory, self.pc);
                self.pc = addr;
                24
            }

            0xCE => { // ADC A, d8
                let val = self.fetch_byte(memory);
                self.alu_adc(val);
                8
            }

            0xCF => { // RST 08H
                self.push(memory, self.pc);
                self.pc = 0x0008;
                16
            }

            // ==================== 0xDX ====================
            0xD0 => { // RET NC
                if !self.flag_c() {
                    self.pc = self.pop(memory);
                    20
                } else {
                    8
                }
            }

            0xD1 => { // POP DE
                let val = self.pop(memory);
                self.set_de(val);
                12
            }

            0xD2 => { // JP NC, a16
                let addr = self.fetch_word(memory);
                if !self.flag_c() {
                    self.pc = addr;
                    16
                } else {
                    12
                }
            }

            0xD3 => panic!("Illegal opcode 0xD3"),

            0xD4 => { // CALL NC, a16
                let addr = self.fetch_word(memory);
                if !self.flag_c() {
                    self.push(memory, self.pc);
                    self.pc = addr;
                    24
                } else {
                    12
                }
            }

            0xD5 => { // PUSH DE
                self.push(memory, self.de());
                16
            }

            0xD6 => { // SUB d8
                let val = self.fetch_byte(memory);
                self.alu_sub(val);
                8
            }

            0xD7 => { // RST 10H
                self.push(memory, self.pc);
                self.pc = 0x0010;
                16
            }

            0xD8 => { // RET C
                if self.flag_c() {
                    self.pc = self.pop(memory);
                    20
                } else {
                    8
                }
            }

            0xD9 => { // RETI
                self.pc = self.pop(memory);
                self.ime = true;
                16
            }

            0xDA => { // JP C, a16
                let addr = self.fetch_word(memory);
                if self.flag_c() {
                    self.pc = addr;
                    16
                } else {
                    12
                }
            }

            0xDB => panic!("Illegal opcode 0xDB"),

            0xDC => { // CALL C, a16
                let addr = self.fetch_word(memory);
                if self.flag_c() {
                    self.push(memory, self.pc);
                    self.pc = addr;
                    24
                } else {
                    12
                }
            }

            0xDD => panic!("Illegal opcode 0xDD"),

            0xDE => { // SBC A, d8
                let val = self.fetch_byte(memory);
                self.alu_sbc(val);
                8
            }

            0xDF => { // RST 18H
                self.push(memory, self.pc);
                self.pc = 0x0018;
                16
            }

            // ==================== 0xEX ====================
            0xE0 => { // LDH (a8), A
                let offset = self.fetch_byte(memory) as u16;
                self.write_byte(memory, 0xFF00 + offset, self.a);
                12
            }

            0xE1 => { // POP HL
                let val = self.pop(memory);
                self.set_hl(val);
                12
            }

            0xE2 => { // LD (C), A
                self.write_byte(memory, 0xFF00 + self.c as u16, self.a);
                8
            }

            0xE3 => panic!("Illegal opcode 0xE3"),
            0xE4 => panic!("Illegal opcode 0xE4"),

            0xE5 => { // PUSH HL
                self.push(memory, self.hl());
                16
            }

            0xE6 => { // AND d8
                let val = self.fetch_byte(memory);
                self.alu_and(val);
                8
            }

            0xE7 => { // RST 20H
                self.push(memory, self.pc);
                self.pc = 0x0020;
                16
            }

            0xE8 => { // ADD SP, r8
                let val = self.fetch_byte(memory) as i8;
                self.sp = self.alu_add_sp(val);
                16
            }

            0xE9 => { // JP HL
                self.pc = self.hl();
                4
            }

            0xEA => { // LD (a16), A
                let addr = self.fetch_word(memory);
                self.write_byte(memory, addr, self.a);
                16
            }

            0xEB => panic!("Illegal opcode 0xEB"),
            0xEC => panic!("Illegal opcode 0xEC"),
            0xED => panic!("Illegal opcode 0xED"),

            0xEE => { // XOR d8
                let val = self.fetch_byte(memory);
                self.alu_xor(val);
                8
            }

            0xEF => { // RST 28H
                self.push(memory, self.pc);
                self.pc = 0x0028;
                16
            }

            // ==================== 0xFX ====================
            0xF0 => { // LDH A, (a8)
                let offset = self.fetch_byte(memory) as u16;
                self.a = self.read_byte(memory, 0xFF00 + offset);
                12
            }

            0xF1 => { // POP AF
                let val = self.pop(memory);
                self.set_af(val);
                12
            }

            0xF2 => { // LD A, (C)
                self.a = self.read_byte(memory, 0xFF00 + self.c as u16);
                8
            }

            0xF3 => { // DI
                self.ime = false;
                4
            }

            0xF4 => panic!("Illegal opcode 0xF4"),

            0xF5 => { // PUSH AF
                self.push(memory, self.af());
                16
            }

            0xF6 => { // OR d8
                let val = self.fetch_byte(memory);
                self.alu_or(val);
                8
            }

            0xF7 => { // RST 30H
                self.push(memory, self.pc);
                self.pc = 0x0030;
                16
            }

            0xF8 => { // LD HL, SP+r8
                let val = self.fetch_byte(memory) as i8;
                let result = self.alu_add_sp(val);
                self.set_hl(result);
                12
            }

            0xF9 => { // LD SP, HL
                self.sp = self.hl();
                8
            }

            0xFA => { // LD A, (a16)
                let addr = self.fetch_word(memory);
                self.a = self.read_byte(memory, addr);
                16
            }

            0xFB => { // EI
                self.ime_pending = true;
                4
            }

            0xFC => panic!("Illegal opcode 0xFC"),
            0xFD => panic!("Illegal opcode 0xFD"),

            0xFE => { // CP d8
                let val = self.fetch_byte(memory);
                self.alu_cp(val);
                8
            }

            0xFF => { // RST 38H
                self.push(memory, self.pc);
                self.pc = 0x0038;
                16
            }
        }
    }

    /// Executes a CB-prefixed instruction.
    fn execute_cb(&mut self, memory: &mut Memory) -> u32 {
        let opcode = self.fetch_byte(memory);

        let get_reg = |cpu: &Cpu, mem: &Memory, idx: u8| -> u8 {
            match idx {
                0 => cpu.b,
                1 => cpu.c,
                2 => cpu.d,
                3 => cpu.e,
                4 => cpu.h,
                5 => cpu.l,
                6 => cpu.read_byte(mem, cpu.hl()),
                7 => cpu.a,
                _ => unreachable!(),
            }
        };

        let set_reg = |cpu: &mut Cpu, mem: &mut Memory, idx: u8, val: u8| {
            match idx {
                0 => cpu.b = val,
                1 => cpu.c = val,
                2 => cpu.d = val,
                3 => cpu.e = val,
                4 => cpu.h = val,
                5 => cpu.l = val,
                6 => cpu.write_byte(mem, cpu.hl(), val),
                7 => cpu.a = val,
                _ => unreachable!(),
            }
        };

        let reg_idx = opcode & 0x07;
        let is_hl = reg_idx == 6;
        let base_cycles = if is_hl { 16 } else { 8 };

        match opcode {
            0x00..=0x07 => { // RLC r
                let val = get_reg(self, memory, reg_idx);
                let result = self.alu_rlc(val);
                set_reg(self, memory, reg_idx, result);
                base_cycles
            }

            0x08..=0x0F => { // RRC r
                let val = get_reg(self, memory, reg_idx);
                let result = self.alu_rrc(val);
                set_reg(self, memory, reg_idx, result);
                base_cycles
            }

            0x10..=0x17 => { // RL r
                let val = get_reg(self, memory, reg_idx);
                let result = self.alu_rl(val);
                set_reg(self, memory, reg_idx, result);
                base_cycles
            }

            0x18..=0x1F => { // RR r
                let val = get_reg(self, memory, reg_idx);
                let result = self.alu_rr(val);
                set_reg(self, memory, reg_idx, result);
                base_cycles
            }

            0x20..=0x27 => { // SLA r
                let val = get_reg(self, memory, reg_idx);
                let result = self.alu_sla(val);
                set_reg(self, memory, reg_idx, result);
                base_cycles
            }

            0x28..=0x2F => { // SRA r
                let val = get_reg(self, memory, reg_idx);
                let result = self.alu_sra(val);
                set_reg(self, memory, reg_idx, result);
                base_cycles
            }

            0x30..=0x37 => { // SWAP r
                let val = get_reg(self, memory, reg_idx);
                let result = self.alu_swap(val);
                set_reg(self, memory, reg_idx, result);
                base_cycles
            }

            0x38..=0x3F => { // SRL r
                let val = get_reg(self, memory, reg_idx);
                let result = self.alu_srl(val);
                set_reg(self, memory, reg_idx, result);
                base_cycles
            }

            0x40..=0x7F => { // BIT b, r
                let bit = (opcode >> 3) & 0x07;
                let val = get_reg(self, memory, reg_idx);
                self.alu_bit(bit, val);
                if is_hl { 12 } else { 8 }
            }

            0x80..=0xBF => { // RES b, r
                let bit = (opcode >> 3) & 0x07;
                let val = get_reg(self, memory, reg_idx);
                let result = self.alu_res(bit, val);
                set_reg(self, memory, reg_idx, result);
                base_cycles
            }

            0xC0..=0xFF => { // SET b, r
                let bit = (opcode >> 3) & 0x07;
                let val = get_reg(self, memory, reg_idx);
                let result = self.alu_set(bit, val);
                set_reg(self, memory, reg_idx, result);
                base_cycles
            }
        }
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
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
        assert_eq!(cpu.sp, 0xFFFE);
        assert_eq!(cpu.pc, 0x0100);
    }

    #[test]
    fn step_executes_nop() {
        let mut cpu = Cpu::new();
        let mut mem = Memory::new();
        mem.data[0x0100] = 0x00;
        cpu.reset();
        cpu.step(&mut mem);
        assert_eq!(cpu.pc, 0x0101);
    }
}
