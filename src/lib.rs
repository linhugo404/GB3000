//! # GB3000 - Game Boy Emulator Core
//!
//! A cycle-accurate Game Boy (DMG) emulator library written in Rust.
//!
//! ## Features
//!
//! - **Full CPU emulation**: All 256 base opcodes and 256 CB-prefixed opcodes
//! - **Accurate timing**: M-cycle accurate CPU execution
//! - **Cycle-exact PPU**: Variable Mode 3 length, STAT interrupt edge detection
//! - **Memory Bank Controllers**: MBC1, MBC2, MBC3, MBC5 support
//! - **Timer**: DIV, TIMA, TMA, TAC with proper interrupt generation
//! - **Audio (APU)**: 4 sound channels (2 pulse, 1 wave, 1 noise)
//! - **Multi-model support**: DMG-0, DMG-ABC, MGB, SGB, SGB2
//!
//! ## Usage
//!
//! ```rust,no_run
//! use gb3000::{Emulator, Button};
//!
//! // Create emulator
//! let mut emulator = Emulator::new();
//!
//! // Load ROM
//! let rom = std::fs::read("game.gb").unwrap();
//! emulator.load_rom(&rom);
//!
//! // Main loop
//! loop {
//!     // Run one frame
//!     emulator.run_frame();
//!
//!     // Get framebuffer (160x144, 2-bit color indices)
//!     let pixels = emulator.framebuffer();
//!
//!     // Get audio samples (stereo f32)
//!     let audio = emulator.audio_samples();
//!
//!     // Update input
//!     emulator.set_button(Button::A, true);
//!
//!     // ... render pixels, play audio ...
//! }
//! ```

pub mod apu;
pub mod cpu;
pub mod memory;
pub mod ppu;
pub mod timer;

use apu::Apu;
use cpu::Cpu;
use memory::{interrupts, Memory};
use ppu::Ppu;
use timer::Timer;

// Re-export commonly used types
pub use cpu::GbModel;
pub use ppu::{SCREEN_HEIGHT, SCREEN_WIDTH};

/// Game Boy button enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    Right,
    Left,
    Up,
    Down,
    A,
    B,
    Select,
    Start,
}

/// ROM information parsed from header
#[derive(Debug, Clone)]
pub struct RomInfo {
    /// Game title (up to 16 characters)
    pub title: String,
    /// Cartridge type description
    pub cart_type: String,
    /// ROM size description
    pub rom_size: String,
    /// RAM size description
    pub ram_size: String,
    /// Cartridge type code
    pub cart_type_code: u8,
    /// ROM size code
    pub rom_size_code: u8,
    /// RAM size code
    pub ram_size_code: u8,
}

/// The main emulator struct
///
/// This is the primary interface for using the emulator. It ties together
/// all the components (CPU, Memory, PPU, APU, Timer) and provides a simple
/// API for running games.
pub struct Emulator {
    cpu: Cpu,
    memory: Memory,
    ppu: Ppu,
    apu: Apu,
    timer: Timer,
    /// Button state (active LOW internally)
    button_state: u8,
}

impl Emulator {
    /// Create a new emulator instance
    pub fn new() -> Self {
        Self {
            cpu: Cpu::new(),
            memory: Memory::new(),
            ppu: Ppu::new(),
            apu: Apu::new(),
            timer: Timer::new(),
            button_state: 0xFF, // All buttons released
        }
    }

    /// Load a ROM into the emulator
    ///
    /// This parses the ROM header and sets up the appropriate memory bank controller.
    pub fn load_rom(&mut self, rom: &[u8]) {
        self.memory.load_rom(rom);
    }

    /// Reset the emulator to initial state
    ///
    /// This resets all components while keeping the ROM loaded.
    pub fn reset(&mut self) {
        self.cpu.reset();
        self.ppu.reset();
        self.apu.reset();
        self.timer.reset();
        self.button_state = 0xFF;
    }

    /// Reset the emulator for a specific hardware model
    pub fn reset_for_model(&mut self, model: GbModel) {
        self.cpu.reset_for_model(model);
        self.ppu.reset();
        self.apu.reset();
        self.timer.reset();
        self.button_state = 0xFF;
    }

    /// Run emulation for one frame (~70224 cycles, ~16.7ms)
    ///
    /// This runs the emulator until VBlank is reached (one complete frame).
    pub fn run_frame(&mut self) {
        const CYCLES_PER_FRAME: u32 = 70224;
        let mut cycles_this_frame = 0u32;

        while cycles_this_frame < CYCLES_PER_FRAME {
            let cycles = self.step();
            cycles_this_frame += cycles;

            if self.ppu.frame_ready {
                self.ppu.frame_ready = false;
                break;
            }
        }
    }

    /// Run emulation for a specific number of cycles
    ///
    /// Useful for more fine-grained control over emulation timing.
    pub fn run_cycles(&mut self, target_cycles: u32) {
        let mut cycles = 0u32;
        while cycles < target_cycles {
            cycles += self.step();
        }
    }

    /// Execute a single CPU instruction and update all subsystems
    ///
    /// Returns the number of T-cycles consumed.
    pub fn step(&mut self) -> u32 {
        // Update joypad state
        self.memory.set_joypad(self.button_state);

        // Handle interrupts
        let intr_cycles = self.handle_interrupts();
        if intr_cycles > 0 {
            self.timer.tick(&mut self.memory, intr_cycles);
            self.ppu.tick(&mut self.memory, intr_cycles);
            self.apu.tick(&mut self.memory, intr_cycles);
            for _ in 0..intr_cycles {
                self.memory.tick_dma();
            }
        }

        // Execute CPU instruction
        let cycles = self.cpu.step(&mut self.memory);

        // Check for PPU register writes that need immediate processing
        if self.memory.stat_written {
            self.memory.stat_written = false;
            self.ppu.on_stat_write(&mut self.memory);
        }
        if self.memory.lyc_written {
            self.memory.lyc_written = false;
            self.ppu.on_lyc_write(&mut self.memory);
        }

        // Update subsystems
        self.timer.tick(&mut self.memory, cycles);
        self.ppu.tick(&mut self.memory, cycles);
        self.apu.tick(&mut self.memory, cycles);

        for _ in 0..cycles {
            self.memory.tick_dma();
        }

        cycles + intr_cycles
    }

    /// Handle pending interrupts
    fn handle_interrupts(&mut self) -> u32 {
        if self.memory.pending_interrupts() != 0 {
            self.cpu.halted = false;
        }

        if !self.cpu.ime {
            return 0;
        }

        let pending = self.memory.pending_interrupts();
        if pending == 0 {
            return 0;
        }

        self.cpu.ime = false;

        let pc = self.cpu.pc;
        self.cpu.sp = self.cpu.sp.wrapping_sub(1);
        self.memory.data[self.cpu.sp as usize] = (pc >> 8) as u8;
        self.cpu.sp = self.cpu.sp.wrapping_sub(1);
        self.memory.data[self.cpu.sp as usize] = pc as u8;

        if pending & interrupts::VBLANK != 0 {
            self.memory.clear_interrupt(interrupts::VBLANK);
            self.cpu.pc = 0x0040;
        } else if pending & interrupts::LCD_STAT != 0 {
            self.memory.clear_interrupt(interrupts::LCD_STAT);
            self.cpu.pc = 0x0048;
        } else if pending & interrupts::TIMER != 0 {
            self.memory.clear_interrupt(interrupts::TIMER);
            self.cpu.pc = 0x0050;
        } else if pending & interrupts::SERIAL != 0 {
            self.memory.clear_interrupt(interrupts::SERIAL);
            self.cpu.pc = 0x0058;
        } else if pending & interrupts::JOYPAD != 0 {
            self.memory.clear_interrupt(interrupts::JOYPAD);
            self.cpu.pc = 0x0060;
        }

        20
    }

    /// Set the state of a button
    ///
    /// # Arguments
    /// * `button` - The button to set
    /// * `pressed` - true if pressed, false if released
    pub fn set_button(&mut self, button: Button, pressed: bool) {
        let bit = match button {
            Button::Right => 0x01,
            Button::Left => 0x02,
            Button::Up => 0x04,
            Button::Down => 0x08,
            Button::A => 0x10,
            Button::B => 0x20,
            Button::Select => 0x40,
            Button::Start => 0x80,
        };

        if pressed {
            self.button_state &= !bit; // Active LOW
        } else {
            self.button_state |= bit;
        }
    }

    /// Get the current framebuffer
    ///
    /// Returns a 160x144 array of 2-bit color indices (0-3).
    /// Use a palette to convert to actual colors.
    pub fn framebuffer(&self) -> &[u8; SCREEN_WIDTH * SCREEN_HEIGHT] {
        &self.ppu.framebuffer
    }

    /// Take pending audio samples from the APU
    ///
    /// Returns stereo interleaved f32 samples at 44100 Hz.
    /// The buffer is cleared after calling this.
    pub fn audio_samples(&mut self) -> Vec<f32> {
        self.apu.take_samples()
    }

    /// Get the audio sample rate
    pub fn audio_sample_rate(&self) -> u32 {
        apu::SAMPLE_RATE
    }

    /// Check if a new frame is ready
    pub fn frame_ready(&self) -> bool {
        self.ppu.frame_ready
    }

    /// Check if the cartridge has battery-backed RAM (saveable)
    pub fn has_battery(&self) -> bool {
        self.memory.has_battery()
    }

    /// Get the external RAM (save data) for battery-backed cartridges
    ///
    /// Returns None if the cartridge has no RAM or no battery.
    pub fn save_ram(&self) -> Option<Vec<u8>> {
        if self.has_battery() {
            Some(self.memory.get_eram().to_vec())
        } else {
            None
        }
    }

    /// Load external RAM (save data) into the cartridge
    ///
    /// Use this to restore a saved game.
    pub fn load_ram(&mut self, data: &[u8]) {
        self.memory.set_eram(data);
    }

    /// Parse ROM information from ROM data
    pub fn parse_rom_info(rom: &[u8]) -> Option<RomInfo> {
        if rom.len() < 0x150 {
            return None;
        }

        let title: String = rom[0x0134..0x0144]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| {
                if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '?'
                }
            })
            .collect();

        let cart_type_code = rom[0x0147];
        let cart_type = match cart_type_code {
            0x00 => "ROM ONLY",
            0x01 => "MBC1",
            0x02 => "MBC1+RAM",
            0x03 => "MBC1+RAM+BATTERY",
            0x05 => "MBC2",
            0x06 => "MBC2+BATTERY",
            0x08 => "ROM+RAM",
            0x09 => "ROM+RAM+BATTERY",
            0x0F => "MBC3+TIMER+BATTERY",
            0x10 => "MBC3+TIMER+RAM+BATTERY",
            0x11 => "MBC3",
            0x12 => "MBC3+RAM",
            0x13 => "MBC3+RAM+BATTERY",
            0x19 => "MBC5",
            0x1A => "MBC5+RAM",
            0x1B => "MBC5+RAM+BATTERY",
            0x1C => "MBC5+RUMBLE",
            0x1D => "MBC5+RUMBLE+RAM",
            0x1E => "MBC5+RUMBLE+RAM+BATTERY",
            _ => "Unknown",
        }
        .to_string();

        let rom_size_code = rom[0x0148];
        let rom_size = match rom_size_code {
            0x00 => "32 KB",
            0x01 => "64 KB",
            0x02 => "128 KB",
            0x03 => "256 KB",
            0x04 => "512 KB",
            0x05 => "1 MB",
            0x06 => "2 MB",
            0x07 => "4 MB",
            0x08 => "8 MB",
            _ => "Unknown",
        }
        .to_string();

        let ram_size_code = rom[0x0149];
        let ram_size = match ram_size_code {
            0x00 => "None",
            0x01 => "2 KB",
            0x02 => "8 KB",
            0x03 => "32 KB",
            0x04 => "128 KB",
            0x05 => "64 KB",
            _ => "Unknown",
        }
        .to_string();

        Some(RomInfo {
            title: title.trim().to_string(),
            cart_type,
            rom_size,
            ram_size,
            cart_type_code,
            rom_size_code,
            ram_size_code,
        })
    }
}

impl Default for Emulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Standard Game Boy palettes
pub mod palettes {
    /// Grayscale palette (White, Light Gray, Dark Gray, Black)
    pub const GRAYSCALE: [u32; 4] = [0xFFFFFFFF, 0xFFAAAAAA, 0xFF555555, 0xFF000000];

    /// Classic DMG green palette
    pub const DMG_GREEN: [u32; 4] = [0xFF9BBC0F, 0xFF8BAC0F, 0xFF306230, 0xFF0F380F];

    /// Game Boy Pocket palette (slightly different green)
    pub const POCKET: [u32; 4] = [0xFFC4CFA1, 0xFF8B956D, 0xFF4D533C, 0xFF1F1F1F];

    /// Game Boy Light palette
    pub const LIGHT: [u32; 4] = [0xFF00B581, 0xFF009A71, 0xFF006839, 0xFF004F2B];

    /// SGB Border palette style
    pub const SGB: [u32; 4] = [0xFFF7E7C6, 0xFFD68E49, 0xFFA63725, 0xFF331820];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emulator_creation() {
        let emu = Emulator::new();
        assert_eq!(emu.framebuffer().len(), SCREEN_WIDTH * SCREEN_HEIGHT);
    }

    #[test]
    fn button_state() {
        let mut emu = Emulator::new();

        // All buttons released
        assert_eq!(emu.button_state, 0xFF);

        // Press A
        emu.set_button(Button::A, true);
        assert_eq!(emu.button_state & 0x10, 0x00);

        // Release A
        emu.set_button(Button::A, false);
        assert_eq!(emu.button_state & 0x10, 0x10);
    }

    #[test]
    fn rom_info_parsing() {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0134..0x0140].copy_from_slice(b"TEST GAME   ");
        rom[0x0147] = 0x01; // MBC1
        rom[0x0148] = 0x00; // 32KB
        rom[0x0149] = 0x00; // No RAM

        let info = Emulator::parse_rom_info(&rom).unwrap();
        assert_eq!(info.title, "TEST GAME");
        assert_eq!(info.cart_type, "MBC1");
        assert_eq!(info.rom_size, "32 KB");
        assert_eq!(info.ram_size, "None");
    }
}

