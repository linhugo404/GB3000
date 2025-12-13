/// Timer subsystem for the Game Boy emulator.
///
/// The Game Boy has a precise timer with the following registers:
/// - DIV (0xFF04): Divider register, upper 8 bits of a 16-bit counter
/// - TIMA (0xFF05): Timer counter, increments based on TAC
/// - TMA (0xFF06): Timer modulo, loaded into TIMA on overflow
/// - TAC (0xFF07): Timer control
///
/// The timer uses falling edge detection on a specific bit of the internal
/// counter (selected by TAC) ANDed with the timer enable bit.

use crate::memory::{io, interrupts, Memory};

/// Timer state for accurate emulation
#[derive(Debug, Clone, Copy, PartialEq)]
enum OverflowState {
    /// Normal operation
    None,
    /// TIMA overflowed, waiting for reload (cycles remaining)
    Pending(u8),
}

#[derive(Debug)]
pub struct Timer {
    /// Internal 16-bit counter (upper 8 bits = DIV register)
    div_counter: u16,
    /// Overflow state for delayed TMA reload
    overflow_state: OverflowState,
    /// TMA value to load when overflow completes
    pending_tma: u8,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            div_counter: 0,
            overflow_state: OverflowState::None,
            pending_tma: 0,
        }
    }

    pub fn reset(&mut self) {
        self.div_counter = 0;
        self.overflow_state = OverflowState::None;
        self.pending_tma = 0;
    }

    /// Get the bit position to check for the given TAC frequency
    fn get_bit_position(tac: u8) -> u8 {
        match tac & 0x03 {
            0 => 9,  // 1024 cycles
            1 => 3,  // 16 cycles
            2 => 5,  // 64 cycles
            3 => 7,  // 256 cycles
            _ => unreachable!(),
        }
    }

    /// Check if the timer clock signal is high
    /// This is the selected bit ANDed with the enable bit
    fn timer_clock_high(&self, tac: u8) -> bool {
        if tac & 0x04 == 0 {
            return false; // Timer disabled
        }
        let bit_pos = Self::get_bit_position(tac);
        (self.div_counter >> bit_pos) & 1 != 0
    }

    /// Advance the timer by the given number of T-cycles.
    pub fn tick(&mut self, memory: &mut Memory, cycles: u32) {
        // Process any pending timer register writes
        self.process_writes(memory);
        
        for _ in 0..cycles {
            self.tick_single(memory);
        }
    }
    
    /// Process timer register writes from memory
    fn process_writes(&mut self, memory: &mut Memory) {
        if memory.timer_div_written {
            memory.timer_div_written = false;
            self.write_div(memory);
        }
        
        if memory.timer_tac_written {
            memory.timer_tac_written = false;
            let old_tac = memory.timer_tac_old_value;
            let new_tac = memory.data[io::TAC as usize];
            self.write_tac(memory, old_tac, new_tac);
        }
        
        if memory.timer_tima_written {
            memory.timer_tima_written = false;
            // Writing to TIMA during overflow window cancels the reload
            if self.in_overflow_window() {
                self.overflow_state = OverflowState::None;
            }
        }
    }

    /// Advance the timer by a single T-cycle.
    fn tick_single(&mut self, memory: &mut Memory) {
        let tac = memory.data[io::TAC as usize];
        let old_clock = self.timer_clock_high(tac);

        // Increment the internal counter
        self.div_counter = self.div_counter.wrapping_add(1);
        
        // Update DIV register
        memory.data[io::DIV as usize] = (self.div_counter >> 8) as u8;

        // Handle overflow state
        match self.overflow_state {
            OverflowState::Pending(1) => {
                // Reload TIMA with TMA and request interrupt
                memory.data[io::TIMA as usize] = memory.data[io::TMA as usize];
                memory.request_interrupt(interrupts::TIMER);
                self.overflow_state = OverflowState::None;
            }
            OverflowState::Pending(n) => {
                self.overflow_state = OverflowState::Pending(n - 1);
            }
            OverflowState::None => {}
        }

        // Check for falling edge
        let new_clock = self.timer_clock_high(tac);
        if old_clock && !new_clock {
            self.increment_tima(memory);
        }
    }

    /// Increment TIMA and handle overflow
    fn increment_tima(&mut self, memory: &mut Memory) {
        let tima = memory.data[io::TIMA as usize];
        let (new_tima, overflow) = tima.overflowing_add(1);
        
        if overflow {
            // TIMA becomes 0, and after 4 cycles it will be reloaded with TMA
            memory.data[io::TIMA as usize] = 0;
            self.overflow_state = OverflowState::Pending(4);
        } else {
            memory.data[io::TIMA as usize] = new_tima;
        }
    }

    /// Called when DIV is written to.
    /// This resets the internal counter and may trigger a TIMA increment.
    pub fn write_div(&mut self, memory: &mut Memory) {
        let tac = memory.data[io::TAC as usize];
        let old_clock = self.timer_clock_high(tac);
        
        // Reset the counter
        self.div_counter = 0;
        memory.data[io::DIV as usize] = 0;
        
        // If the clock was high and is now low, increment TIMA
        if old_clock {
            self.increment_tima(memory);
        }
    }

    /// Called when TAC is written to.
    /// Changing frequency or disabling can trigger a TIMA increment.
    pub fn write_tac(&mut self, memory: &mut Memory, old_tac: u8, new_tac: u8) {
        let old_clock = if old_tac & 0x04 != 0 {
            let bit_pos = Self::get_bit_position(old_tac);
            (self.div_counter >> bit_pos) & 1 != 0
        } else {
            false
        };

        let new_clock = if new_tac & 0x04 != 0 {
            let bit_pos = Self::get_bit_position(new_tac);
            (self.div_counter >> bit_pos) & 1 != 0
        } else {
            false
        };

        // If clock goes from high to low, increment TIMA
        if old_clock && !new_clock {
            self.increment_tima(memory);
        }
    }

    /// Called when TIMA is written to during the overflow period.
    /// Writing to TIMA during the 4-cycle window cancels the TMA reload.
    pub fn write_tima(&mut self, memory: &mut Memory, value: u8) {
        memory.data[io::TIMA as usize] = value;
        // Cancel any pending overflow
        self.overflow_state = OverflowState::None;
    }

    /// Check if we're in the overflow window (for detecting writes)
    pub fn in_overflow_window(&self) -> bool {
        matches!(self.overflow_state, OverflowState::Pending(_))
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

        // TIMA should become 0 immediately after overflow
        // Then after 4 more cycles it gets TMA value
        timer.tick(&mut memory, 4);
        
        // TIMA should have been reloaded with TMA
        assert_eq!(memory.data[io::TIMA as usize], 0x42);

        // Timer interrupt should be requested
        assert!(memory.data[io::IF as usize] & interrupts::TIMER != 0);
    }
}

