/// Timer subsystem for the Game Boy emulator.
///
/// The Game Boy has a simple timer with the following registers:
/// - DIV (0xFF04): Divider register, increments at 16384Hz
/// - TIMA (0xFF05): Timer counter, increments at rate set by TAC
/// - TMA (0xFF06): Timer modulo, loaded into TIMA on overflow
/// - TAC (0xFF07): Timer control

use crate::memory::{io, interrupts, Memory};

/// Timer frequencies (in CPU cycles per increment)
const TIMER_FREQUENCIES: [u32; 4] = [
    1024, // 4096 Hz (CPU clock / 1024)
    16,   // 262144 Hz (CPU clock / 16)
    64,   // 65536 Hz (CPU clock / 64)
    256,  // 16384 Hz (CPU clock / 256)
];

#[derive(Debug)]
pub struct Timer {
    /// Internal divider counter (16-bit, upper 8 bits = DIV register)
    div_counter: u16,
    /// Timer counter for TIMA
    timer_counter: u32,
    /// Previous state of the selected bit (for falling edge detection)
    prev_bit: bool,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            div_counter: 0,
            timer_counter: 0,
            prev_bit: false,
        }
    }

    pub fn reset(&mut self) {
        self.div_counter = 0;
        self.timer_counter = 0;
        self.prev_bit = false;
    }

    /// Advance the timer by the given number of T-cycles.
    pub fn tick(&mut self, memory: &mut Memory, cycles: u32) {
        for _ in 0..cycles {
            self.tick_single(memory);
        }
    }

    /// Advance the timer by a single T-cycle.
    fn tick_single(&mut self, memory: &mut Memory) {
        // DIV increments every 256 T-cycles (it's the upper 8 bits of a 16-bit counter)
        self.div_counter = self.div_counter.wrapping_add(1);
        memory.data[io::DIV as usize] = (self.div_counter >> 8) as u8;

        // Check if timer is enabled
        let tac = memory.data[io::TAC as usize];
        let timer_enabled = tac & 0x04 != 0;

        if timer_enabled {
            // Determine which bit of div_counter to check based on frequency
            let freq_select = (tac & 0x03) as usize;
            let bit_position = match freq_select {
                0 => 9,  // Check bit 9 (1024 cycles)
                1 => 3,  // Check bit 3 (16 cycles)
                2 => 5,  // Check bit 5 (64 cycles)
                3 => 7,  // Check bit 7 (256 cycles)
                _ => unreachable!(),
            };

            // Get the current state of the selected bit
            let current_bit = (self.div_counter >> bit_position) & 1 != 0;

            // Falling edge detection - increment TIMA when bit goes from 1 to 0
            if self.prev_bit && !current_bit {
                let tima = memory.data[io::TIMA as usize];
                let (new_tima, overflow) = tima.overflowing_add(1);

                if overflow {
                    // Reload TIMA with TMA and request timer interrupt
                    memory.data[io::TIMA as usize] = memory.data[io::TMA as usize];
                    memory.request_interrupt(interrupts::TIMER);
                } else {
                    memory.data[io::TIMA as usize] = new_tima;
                }
            }

            self.prev_bit = current_bit;
        } else {
            self.prev_bit = false;
        }
    }

    /// Called when DIV is written to (resets the divider counter)
    pub fn reset_div(&mut self) {
        self.div_counter = 0;
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn div_increments() {
        let mut timer = Timer::new();
        let mut memory = Memory::new();

        // DIV should be 0 initially
        assert_eq!(memory.data[io::DIV as usize], 0);

        // After 256 cycles, DIV should be 1
        timer.tick(&mut memory, 256);
        assert_eq!(memory.data[io::DIV as usize], 1);

        // After another 256 cycles, DIV should be 2
        timer.tick(&mut memory, 256);
        assert_eq!(memory.data[io::DIV as usize], 2);
    }

    #[test]
    fn timer_interrupt_on_overflow() {
        let mut timer = Timer::new();
        let mut memory = Memory::new();

        // Enable timer with fastest frequency (16 cycles per increment)
        memory.data[io::TAC as usize] = 0x05; // Enabled, freq = 01

        // Set TIMA to 0xFF so it will overflow soon
        memory.data[io::TIMA as usize] = 0xFF;
        memory.data[io::TMA as usize] = 0x42;

        // Clear interrupt flags
        memory.data[io::IF as usize] = 0;

        // Tick until overflow (need 16 cycles for one increment)
        timer.tick(&mut memory, 16);

        // TIMA should have been reloaded with TMA
        assert_eq!(memory.data[io::TIMA as usize], 0x42);

        // Timer interrupt should be requested
        assert!(memory.data[io::IF as usize] & interrupts::TIMER != 0);
    }
}

