/// Memory subsystem for the Game Boy emulator.
///
/// The Game Boy has a 16-bit address space (64KB) with the following layout:
/// - 0x0000-0x3FFF: ROM Bank 0 (16KB)
/// - 0x4000-0x7FFF: ROM Bank 1-N (switchable, 16KB)
/// - 0x8000-0x9FFF: Video RAM (8KB)
/// - 0xA000-0xBFFF: External RAM (8KB, switchable)
/// - 0xC000-0xDFFF: Work RAM (8KB)
/// - 0xE000-0xFDFF: Echo RAM (mirror of C000-DDFF)
/// - 0xFE00-0xFE9F: OAM (Sprite Attribute Table)
/// - 0xFEA0-0xFEFF: Not usable
/// - 0xFF00-0xFF7F: I/O Registers
/// - 0xFF80-0xFFFE: High RAM (HRAM)
/// - 0xFFFF: Interrupt Enable Register

/// Hardware register addresses
pub mod io {
    // Joypad
    pub const JOYP: u16 = 0xFF00;
    
    // Serial
    pub const SB: u16 = 0xFF01;
    pub const SC: u16 = 0xFF02;
    
    // Timer
    pub const DIV: u16 = 0xFF04;
    pub const TIMA: u16 = 0xFF05;
    pub const TMA: u16 = 0xFF06;
    pub const TAC: u16 = 0xFF07;
    
    // Interrupts
    pub const IF: u16 = 0xFF0F;
    pub const IE: u16 = 0xFFFF;
    
    // Sound
    pub const NR10: u16 = 0xFF10;
    pub const NR11: u16 = 0xFF11;
    pub const NR12: u16 = 0xFF12;
    pub const NR13: u16 = 0xFF13;
    pub const NR14: u16 = 0xFF14;
    pub const NR21: u16 = 0xFF16;
    pub const NR22: u16 = 0xFF17;
    pub const NR23: u16 = 0xFF18;
    pub const NR24: u16 = 0xFF19;
    pub const NR30: u16 = 0xFF1A;
    pub const NR31: u16 = 0xFF1B;
    pub const NR32: u16 = 0xFF1C;
    pub const NR33: u16 = 0xFF1D;
    pub const NR34: u16 = 0xFF1E;
    pub const NR41: u16 = 0xFF20;
    pub const NR42: u16 = 0xFF21;
    pub const NR43: u16 = 0xFF22;
    pub const NR44: u16 = 0xFF23;
    pub const NR50: u16 = 0xFF24;
    pub const NR51: u16 = 0xFF25;
    pub const NR52: u16 = 0xFF26;
    
    // PPU
    pub const LCDC: u16 = 0xFF40;
    pub const STAT: u16 = 0xFF41;
    pub const SCY: u16 = 0xFF42;
    pub const SCX: u16 = 0xFF43;
    pub const LY: u16 = 0xFF44;
    pub const LYC: u16 = 0xFF45;
    pub const DMA: u16 = 0xFF46;
    pub const BGP: u16 = 0xFF47;
    pub const OBP0: u16 = 0xFF48;
    pub const OBP1: u16 = 0xFF49;
    pub const WY: u16 = 0xFF4A;
    pub const WX: u16 = 0xFF4B;
}

/// Interrupt flag bits
pub mod interrupts {
    pub const VBLANK: u8 = 0b0000_0001;
    pub const LCD_STAT: u8 = 0b0000_0010;
    pub const TIMER: u8 = 0b0000_0100;
    pub const SERIAL: u8 = 0b0000_1000;
    pub const JOYPAD: u8 = 0b0001_0000;
}

#[derive(Debug)]
pub struct Memory {
    /// Raw memory array (64KB)
    pub data: [u8; 0x10000],
    /// ROM data (can be larger than 32KB for banked ROMs)
    rom: Vec<u8>,
    /// External RAM
    eram: Vec<u8>,
    /// Current ROM bank (for MBC)
    rom_bank: u16,
    /// Current RAM bank (for MBC)
    ram_bank: u8,
    /// RAM enabled flag (for MBC)
    ram_enabled: bool,
    /// MBC type
    mbc_type: MbcType,
    /// Banking mode (for MBC1)
    banking_mode: u8,
    /// Joypad state (directly accessible for input handling)
    pub joypad_state: u8,
    /// DMA transfer in progress
    dma_active: bool,
    dma_source: u16,
    dma_offset: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MbcType {
    None,
    Mbc1,
    Mbc2,
    Mbc3,
    Mbc5,
}

impl Memory {
    /// Creates new memory initialized to zero.
    pub fn new() -> Self {
        let mut mem = Self {
            data: [0; 0x10000],
            rom: Vec::new(),
            eram: vec![0; 0x8000], // 32KB max external RAM
            rom_bank: 1,
            ram_bank: 0,
            ram_enabled: false,
            mbc_type: MbcType::None,
            banking_mode: 0,
            joypad_state: 0xFF, // All buttons released
            dma_active: false,
            dma_source: 0,
            dma_offset: 0,
        };
        // Initialize some registers to their power-on values
        mem.data[io::LCDC as usize] = 0x91;
        mem.data[io::BGP as usize] = 0xFC;
        mem.data[io::OBP0 as usize] = 0xFF;
        mem.data[io::OBP1 as usize] = 0xFF;
        mem.data[io::JOYP as usize] = 0xCF;
        mem
    }

    /// Loads the given ROM bytes and detects cartridge type.
    pub fn load_rom(&mut self, rom: &[u8]) {
        self.rom = rom.to_vec();
        
        // Copy first 32KB to memory
        let len = rom.len().min(0x8000);
        self.data[..len].copy_from_slice(&rom[..len]);
        
        // Detect MBC type from cartridge header (0x0147)
        if rom.len() > 0x0147 {
            self.mbc_type = match rom[0x0147] {
                0x00 => MbcType::None,
                0x01..=0x03 => MbcType::Mbc1,
                0x05..=0x06 => MbcType::Mbc2,
                0x0F..=0x13 => MbcType::Mbc3,
                0x19..=0x1E => MbcType::Mbc5,
                _ => MbcType::None,
            };
        }
        
        // Determine RAM size from header (0x0149)
        if rom.len() > 0x0149 {
            let ram_size = match rom[0x0149] {
                0x00 => 0,
                0x01 => 0x800,    // 2KB
                0x02 => 0x2000,   // 8KB
                0x03 => 0x8000,   // 32KB
                0x04 => 0x20000,  // 128KB
                0x05 => 0x10000,  // 64KB
                _ => 0x8000,
            };
            if ram_size > 0 {
                self.eram = vec![0; ram_size];
            }
        }
    }

    /// Reads a byte from the given address.
    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            // ROM Bank 0
            0x0000..=0x3FFF => {
                if self.mbc_type == MbcType::Mbc1 && self.banking_mode == 1 {
                    let bank = (self.ram_bank as usize) << 5;
                    let offset = (bank * 0x4000) + (addr as usize);
                    self.rom.get(offset).copied().unwrap_or(0xFF)
                } else {
                    self.rom.get(addr as usize).copied().unwrap_or(0xFF)
                }
            }
            
            // ROM Bank 1-N (switchable)
            0x4000..=0x7FFF => {
                let bank = self.rom_bank as usize;
                let offset = (bank * 0x4000) + ((addr as usize) - 0x4000);
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            
            // External RAM
            0xA000..=0xBFFF => {
                if self.ram_enabled {
                    let bank = self.ram_bank as usize;
                    let offset = (bank * 0x2000) + ((addr as usize) - 0xA000);
                    self.eram.get(offset).copied().unwrap_or(0xFF)
                } else {
                    0xFF
                }
            }
            
            // Echo RAM
            0xE000..=0xFDFF => self.data[(addr - 0x2000) as usize],
            
            // Joypad register
            0xFF00 => self.read_joypad(),
            
            // Not usable area
            0xFEA0..=0xFEFF => 0xFF,
            
            // Everything else reads from data array
            _ => self.data[addr as usize],
        }
    }

    /// Writes a byte to the given address.
    pub fn write_byte(&mut self, addr: u16, value: u8) {
        match addr {
            // ROM area - MBC register writes
            0x0000..=0x1FFF => {
                // RAM enable
                match self.mbc_type {
                    MbcType::Mbc1 | MbcType::Mbc3 | MbcType::Mbc5 => {
                        self.ram_enabled = (value & 0x0F) == 0x0A;
                    }
                    MbcType::Mbc2 => {
                        if addr & 0x0100 == 0 {
                            self.ram_enabled = (value & 0x0F) == 0x0A;
                        }
                    }
                    MbcType::None => {}
                }
            }
            
            0x2000..=0x3FFF => {
                // ROM bank select
                match self.mbc_type {
                    MbcType::Mbc1 => {
                        let bank = value & 0x1F;
                        self.rom_bank = (self.rom_bank & 0x60) | (bank as u16);
                        if self.rom_bank == 0 {
                            self.rom_bank = 1;
                        }
                    }
                    MbcType::Mbc2 => {
                        if addr & 0x0100 != 0 {
                            self.rom_bank = (value & 0x0F) as u16;
                            if self.rom_bank == 0 {
                                self.rom_bank = 1;
                            }
                        }
                    }
                    MbcType::Mbc3 => {
                        let bank = value & 0x7F;
                        self.rom_bank = if bank == 0 { 1 } else { bank as u16 };
                    }
                    MbcType::Mbc5 => {
                        if addr < 0x3000 {
                            self.rom_bank = (self.rom_bank & 0x100) | (value as u16);
                        } else {
                            self.rom_bank = (self.rom_bank & 0xFF) | (((value & 1) as u16) << 8);
                        }
                    }
                    MbcType::None => {}
                }
            }
            
            0x4000..=0x5FFF => {
                // RAM bank select (or upper bits of ROM bank for MBC1)
                match self.mbc_type {
                    MbcType::Mbc1 => {
                        self.ram_bank = value & 0x03;
                        if self.banking_mode == 0 {
                            self.rom_bank = (self.rom_bank & 0x1F) | (((value & 0x03) as u16) << 5);
                        }
                    }
                    MbcType::Mbc3 => {
                        self.ram_bank = value & 0x0F;
                    }
                    MbcType::Mbc5 => {
                        self.ram_bank = value & 0x0F;
                    }
                    _ => {}
                }
            }
            
            0x6000..=0x7FFF => {
                // Banking mode select (MBC1 only)
                if self.mbc_type == MbcType::Mbc1 {
                    self.banking_mode = value & 0x01;
                }
            }
            
            // VRAM
            0x8000..=0x9FFF => {
                self.data[addr as usize] = value;
            }
            
            // External RAM
            0xA000..=0xBFFF => {
                if self.ram_enabled {
                    let bank = self.ram_bank as usize;
                    let offset = (bank * 0x2000) + ((addr as usize) - 0xA000);
                    if offset < self.eram.len() {
                        self.eram[offset] = value;
                    }
                }
            }
            
            // Work RAM
            0xC000..=0xDFFF => {
                self.data[addr as usize] = value;
            }
            
            // Echo RAM
            0xE000..=0xFDFF => {
                self.data[(addr - 0x2000) as usize] = value;
            }
            
            // OAM
            0xFE00..=0xFE9F => {
                self.data[addr as usize] = value;
            }
            
            // Not usable
            0xFEA0..=0xFEFF => {}
            
            // I/O Registers
            0xFF00..=0xFF7F => {
                self.write_io(addr, value);
            }
            
            // HRAM
            0xFF80..=0xFFFE => {
                self.data[addr as usize] = value;
            }
            
            // IE register
            0xFFFF => {
                self.data[addr as usize] = value;
            }
        }
    }

    /// Reads the joypad register with proper button/direction selection
    fn read_joypad(&self) -> u8 {
        let select = self.data[io::JOYP as usize];
        let mut result = select | 0x0F;
        
        // Buttons are active low
        if select & 0x20 == 0 {
            // Select button keys
            result &= (self.joypad_state >> 4) | 0xF0;
        }
        if select & 0x10 == 0 {
            // Select direction keys
            result &= (self.joypad_state & 0x0F) | 0xF0;
        }
        
        result | 0xC0 // Upper bits always 1
    }

    /// Handles I/O register writes
    fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            io::JOYP => {
                // Only bits 4-5 are writable
                self.data[addr as usize] = (value & 0x30) | (self.data[addr as usize] & 0xCF);
            }
            
            io::DIV => {
                // Writing any value resets DIV to 0
                self.data[addr as usize] = 0;
            }
            
            io::DMA => {
                // Start DMA transfer
                self.dma_source = (value as u16) << 8;
                self.dma_active = true;
                self.dma_offset = 0;
                self.data[addr as usize] = value;
            }
            
            io::LY => {
                // LY is read-only, writes are ignored
            }
            
            io::STAT => {
                // Lower 3 bits are read-only
                self.data[addr as usize] = (value & 0xF8) | (self.data[addr as usize] & 0x07);
            }
            
            _ => {
                self.data[addr as usize] = value;
            }
        }
    }

    /// Performs one step of DMA transfer (if active)
    pub fn tick_dma(&mut self) {
        if self.dma_active {
            let src = self.dma_source + self.dma_offset as u16;
            let dst = 0xFE00 + self.dma_offset as u16;
            let val = self.read_byte(src);
            self.data[dst as usize] = val;
            
            self.dma_offset += 1;
            if self.dma_offset >= 160 {
                self.dma_active = false;
            }
        }
    }

    /// Request an interrupt
    pub fn request_interrupt(&mut self, interrupt: u8) {
        self.data[io::IF as usize] |= interrupt;
    }

    /// Get pending interrupts (IF & IE)
    pub fn pending_interrupts(&self) -> u8 {
        self.data[io::IF as usize] & self.data[io::IE as usize] & 0x1F
    }

    /// Clear an interrupt flag
    pub fn clear_interrupt(&mut self, interrupt: u8) {
        self.data[io::IF as usize] &= !interrupt;
    }

    /// Set joypad button state (bit = 0 means pressed)
    /// Bits: 7-4 = Start, Select, B, A | 3-0 = Down, Up, Left, Right
    pub fn set_joypad(&mut self, state: u8) {
        let old_state = self.joypad_state;
        self.joypad_state = state;
        
        // Request joypad interrupt on any button press (high to low transition)
        if (old_state & !state) != 0 {
            self.request_interrupt(interrupts::JOYPAD);
        }
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
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

    #[test]
    fn load_rom_copies_bytes() {
        let rom = vec![0xAA, 0xBB, 0xCC];
        let mut mem = Memory::new();
        mem.load_rom(&rom);
        assert_eq!(mem.read_byte(0x0000), 0xAA);
        assert_eq!(mem.read_byte(0x0001), 0xBB);
        assert_eq!(mem.read_byte(0x0002), 0xCC);
    }

    #[test]
    fn echo_ram_mirrors_wram() {
        let mut mem = Memory::new();
        mem.write_byte(0xC000, 0x55);
        assert_eq!(mem.read_byte(0xE000), 0x55);
    }

    #[test]
    fn div_reset_on_write() {
        let mut mem = Memory::new();
        mem.data[io::DIV as usize] = 0xAB;
        mem.write_byte(io::DIV, 0x12);
        assert_eq!(mem.read_byte(io::DIV), 0x00);
    }

    #[test]
    fn interrupt_request_and_clear() {
        let mut mem = Memory::new();
        mem.request_interrupt(interrupts::VBLANK);
        assert_eq!(mem.data[io::IF as usize] & interrupts::VBLANK, interrupts::VBLANK);
        mem.clear_interrupt(interrupts::VBLANK);
        assert_eq!(mem.data[io::IF as usize] & interrupts::VBLANK, 0);
    }
}
