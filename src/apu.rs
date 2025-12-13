/// Audio Processing Unit (APU) for the Game Boy emulator.
///
/// The Game Boy has 4 sound channels:
/// - Channel 1: Pulse with sweep
/// - Channel 2: Pulse
/// - Channel 3: Wave
/// - Channel 4: Noise
///
/// This is a basic implementation that generates audio samples.

use crate::memory::{io, Memory};

/// Audio sample rate
pub const SAMPLE_RATE: u32 = 44100;

/// CPU cycles per audio sample
const CYCLES_PER_SAMPLE: u32 = 4194304 / SAMPLE_RATE;

/// Frame sequencer step period (in CPU cycles)
const FRAME_SEQUENCER_PERIOD: u32 = 8192;

#[derive(Debug)]
pub struct Apu {
    /// Cycle counter for sample generation
    sample_counter: u32,
    /// Cycle counter for frame sequencer
    frame_counter: u32,
    /// Frame sequencer step (0-7)
    frame_step: u8,
    /// Audio buffer
    pub buffer: Vec<f32>,
    /// Audio enabled flag
    enabled: bool,
    /// High-pass filter state for left/right channels (removes DC offset and reduces pops)
    hpf_left: f32,
    hpf_right: f32,

    // Channel 1 (Pulse with sweep)
    ch1_enabled: bool,
    ch1_dac_enabled: bool,
    ch1_length_counter: u8,
    ch1_length_enabled: bool,
    ch1_frequency: u16,
    ch1_timer: u16,
    ch1_duty_position: u8,
    ch1_volume: u8,
    ch1_volume_initial: u8,
    ch1_envelope_timer: u8,
    ch1_envelope_period: u8,
    ch1_envelope_add: bool,
    ch1_sweep_period: u8,
    ch1_sweep_shift: u8,
    ch1_sweep_negate: bool,
    ch1_sweep_timer: u8,
    ch1_sweep_enabled: bool,
    ch1_sweep_shadow: u16,

    // Channel 2 (Pulse)
    ch2_enabled: bool,
    ch2_dac_enabled: bool,
    ch2_length_counter: u8,
    ch2_length_enabled: bool,
    ch2_frequency: u16,
    ch2_timer: u16,
    ch2_duty_position: u8,
    ch2_volume: u8,
    ch2_volume_initial: u8,
    ch2_envelope_timer: u8,
    ch2_envelope_period: u8,
    ch2_envelope_add: bool,

    // Channel 3 (Wave)
    ch3_enabled: bool,
    ch3_dac_enabled: bool,
    ch3_length_counter: u16,
    ch3_length_enabled: bool,
    ch3_frequency: u16,
    ch3_timer: u16,
    ch3_position: u8,
    ch3_volume_code: u8,
    ch3_sample_buffer: u8,

    // Channel 4 (Noise)
    ch4_enabled: bool,
    ch4_dac_enabled: bool,
    ch4_length_counter: u8,
    ch4_length_enabled: bool,
    ch4_volume: u8,
    ch4_volume_initial: u8,
    ch4_envelope_timer: u8,
    ch4_envelope_period: u8,
    ch4_envelope_add: bool,
    ch4_timer: u32,
    ch4_lfsr: u16,
    ch4_width_mode: bool,
    ch4_clock_shift: u8,
    ch4_divisor_code: u8,
}

/// Duty cycle patterns for pulse channels
const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1], // 12.5%
    [1, 0, 0, 0, 0, 0, 0, 1], // 25%
    [1, 0, 0, 0, 0, 1, 1, 1], // 50%
    [0, 1, 1, 1, 1, 1, 1, 0], // 75%
];

impl Apu {
    pub fn new() -> Self {
        Self {
            sample_counter: 0,
            frame_counter: 0,
            frame_step: 0,
            buffer: Vec::with_capacity(1024),
            enabled: false,
            hpf_left: 0.0,
            hpf_right: 0.0,

            ch1_enabled: false,
            ch1_dac_enabled: false,
            ch1_length_counter: 0,
            ch1_length_enabled: false,
            ch1_frequency: 0,
            ch1_timer: 0,
            ch1_duty_position: 0,
            ch1_volume: 0,
            ch1_volume_initial: 0,
            ch1_envelope_timer: 0,
            ch1_envelope_period: 0,
            ch1_envelope_add: false,
            ch1_sweep_period: 0,
            ch1_sweep_shift: 0,
            ch1_sweep_negate: false,
            ch1_sweep_timer: 0,
            ch1_sweep_enabled: false,
            ch1_sweep_shadow: 0,

            ch2_enabled: false,
            ch2_dac_enabled: false,
            ch2_length_counter: 0,
            ch2_length_enabled: false,
            ch2_frequency: 0,
            ch2_timer: 0,
            ch2_duty_position: 0,
            ch2_volume: 0,
            ch2_volume_initial: 0,
            ch2_envelope_timer: 0,
            ch2_envelope_period: 0,
            ch2_envelope_add: false,

            ch3_enabled: false,
            ch3_dac_enabled: false,
            ch3_length_counter: 0,
            ch3_length_enabled: false,
            ch3_frequency: 0,
            ch3_timer: 0,
            ch3_position: 0,
            ch3_volume_code: 0,
            ch3_sample_buffer: 0,

            ch4_enabled: false,
            ch4_dac_enabled: false,
            ch4_length_counter: 0,
            ch4_length_enabled: false,
            ch4_volume: 0,
            ch4_volume_initial: 0,
            ch4_envelope_timer: 0,
            ch4_envelope_period: 0,
            ch4_envelope_add: false,
            ch4_timer: 0,
            ch4_lfsr: 0x7FFF,
            ch4_width_mode: false,
            ch4_clock_shift: 0,
            ch4_divisor_code: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Tick the APU by the given number of T-cycles
    pub fn tick(&mut self, memory: &mut Memory, cycles: u32) {
        // Update enabled state from NR52
        self.enabled = memory.data[io::NR52 as usize] & 0x80 != 0;

        if !self.enabled {
            return;
        }

        // Read channel parameters from memory (and handle triggers)
        self.read_channel_registers(memory);

        for _ in 0..cycles {
            // Tick channels
            self.tick_channel1();
            self.tick_channel2();
            self.tick_channel3(memory);
            self.tick_channel4();

            // Frame sequencer
            self.frame_counter += 1;
            if self.frame_counter >= FRAME_SEQUENCER_PERIOD {
                self.frame_counter = 0;
                self.tick_frame_sequencer();
            }

            // Generate sample
            self.sample_counter += 1;
            if self.sample_counter >= CYCLES_PER_SAMPLE {
                self.sample_counter = 0;
                self.generate_sample_output(memory);
            }
        }
    }

    fn read_channel_registers(&mut self, memory: &mut Memory) {
        // Channel 1
        let nr10 = memory.data[io::NR10 as usize];
        self.ch1_sweep_period = (nr10 >> 4) & 0x07;
        self.ch1_sweep_negate = nr10 & 0x08 != 0;
        self.ch1_sweep_shift = nr10 & 0x07;

        let nr11 = memory.data[io::NR11 as usize];
        let nr12 = memory.data[io::NR12 as usize];
        self.ch1_dac_enabled = nr12 & 0xF8 != 0;

        let nr13 = memory.data[io::NR13 as usize];
        let nr14 = memory.data[io::NR14 as usize];
        self.ch1_frequency = (nr13 as u16) | (((nr14 & 0x07) as u16) << 8);
        self.ch1_length_enabled = nr14 & 0x40 != 0;
        
        // Check for channel 1 trigger
        if nr14 & 0x80 != 0 {
            memory.data[io::NR14 as usize] &= 0x7F; // Clear trigger bit
            if self.ch1_dac_enabled {
                self.ch1_enabled = true;
                self.ch1_length_counter = 64 - (nr11 & 0x3F);
                self.ch1_timer = (2048 - self.ch1_frequency) * 4;
                self.ch1_volume = nr12 >> 4;
                self.ch1_envelope_timer = nr12 & 0x07;
                self.ch1_envelope_period = nr12 & 0x07;
                self.ch1_envelope_add = nr12 & 0x08 != 0;
                self.ch1_sweep_shadow = self.ch1_frequency;
                self.ch1_sweep_timer = if self.ch1_sweep_period > 0 { self.ch1_sweep_period } else { 8 };
                self.ch1_sweep_enabled = self.ch1_sweep_period > 0 || self.ch1_sweep_shift > 0;
            }
        }

        // Channel 2
        let nr21 = memory.data[io::NR21 as usize];
        let nr22 = memory.data[io::NR22 as usize];
        self.ch2_dac_enabled = nr22 & 0xF8 != 0;

        let nr23 = memory.data[io::NR23 as usize];
        let nr24 = memory.data[io::NR24 as usize];
        self.ch2_frequency = (nr23 as u16) | (((nr24 & 0x07) as u16) << 8);
        self.ch2_length_enabled = nr24 & 0x40 != 0;
        
        // Check for channel 2 trigger
        if nr24 & 0x80 != 0 {
            memory.data[io::NR24 as usize] &= 0x7F; // Clear trigger bit
            if self.ch2_dac_enabled {
                self.ch2_enabled = true;
                self.ch2_length_counter = 64 - (nr21 & 0x3F);
                self.ch2_timer = (2048 - self.ch2_frequency) * 4;
                self.ch2_volume = nr22 >> 4;
                self.ch2_envelope_timer = nr22 & 0x07;
                self.ch2_envelope_period = nr22 & 0x07;
                self.ch2_envelope_add = nr22 & 0x08 != 0;
            }
        }

        // Channel 3
        let nr30 = memory.data[io::NR30 as usize];
        self.ch3_dac_enabled = nr30 & 0x80 != 0;

        let nr31 = memory.data[io::NR31 as usize];
        let nr32 = memory.data[io::NR32 as usize];
        self.ch3_volume_code = (nr32 >> 5) & 0x03;

        let nr33 = memory.data[io::NR33 as usize];
        let nr34 = memory.data[io::NR34 as usize];
        self.ch3_frequency = (nr33 as u16) | (((nr34 & 0x07) as u16) << 8);
        self.ch3_length_enabled = nr34 & 0x40 != 0;
        
        // Check for channel 3 trigger
        if nr34 & 0x80 != 0 {
            memory.data[io::NR34 as usize] &= 0x7F; // Clear trigger bit
            if self.ch3_dac_enabled {
                self.ch3_enabled = true;
                self.ch3_length_counter = 256 - (nr31 as u16);
                self.ch3_timer = (2048 - self.ch3_frequency) * 2;
                self.ch3_position = 0;
            }
        }

        // Channel 4
        let nr41 = memory.data[io::NR41 as usize];
        let nr42 = memory.data[io::NR42 as usize];
        self.ch4_dac_enabled = nr42 & 0xF8 != 0;

        let nr43 = memory.data[io::NR43 as usize];
        self.ch4_clock_shift = nr43 >> 4;
        self.ch4_width_mode = nr43 & 0x08 != 0;
        
        let nr44 = memory.data[io::NR44 as usize];
        self.ch4_length_enabled = nr44 & 0x40 != 0;
        
        // Check for channel 4 trigger
        if nr44 & 0x80 != 0 {
            memory.data[io::NR44 as usize] &= 0x7F; // Clear trigger bit
            if self.ch4_dac_enabled {
                self.ch4_enabled = true;
                self.ch4_length_counter = 64 - (nr41 & 0x3F);
                self.ch4_lfsr = 0x7FFF;
                self.ch4_volume = nr42 >> 4;
                self.ch4_envelope_timer = nr42 & 0x07;
                self.ch4_envelope_period = nr42 & 0x07;
                self.ch4_envelope_add = nr42 & 0x08 != 0;
                let divisor: u32 = if nr43 & 0x07 == 0 { 8 } else { (nr43 & 0x07) as u32 * 16 };
                self.ch4_timer = divisor << self.ch4_clock_shift;
            }
        }
        self.ch4_divisor_code = nr43 & 0x07;
    }

    fn tick_channel1(&mut self) {
        if self.ch1_timer > 0 {
            self.ch1_timer -= 1;
        }
        if self.ch1_timer == 0 {
            self.ch1_timer = (2048 - self.ch1_frequency) * 4;
            self.ch1_duty_position = (self.ch1_duty_position + 1) % 8;
        }
    }

    fn tick_channel2(&mut self) {
        if self.ch2_timer > 0 {
            self.ch2_timer -= 1;
        }
        if self.ch2_timer == 0 {
            self.ch2_timer = (2048 - self.ch2_frequency) * 4;
            self.ch2_duty_position = (self.ch2_duty_position + 1) % 8;
        }
    }

    fn tick_channel3(&mut self, memory: &Memory) {
        if self.ch3_timer > 0 {
            self.ch3_timer -= 1;
        }
        if self.ch3_timer == 0 {
            self.ch3_timer = (2048 - self.ch3_frequency) * 2;
            self.ch3_position = (self.ch3_position + 1) % 32;

            // Read sample from wave RAM
            let addr = 0xFF30 + (self.ch3_position / 2) as u16;
            let byte = memory.data[addr as usize];
            self.ch3_sample_buffer = if self.ch3_position % 2 == 0 {
                byte >> 4
            } else {
                byte & 0x0F
            };
        }
    }

    fn tick_channel4(&mut self) {
        if self.ch4_timer > 0 {
            self.ch4_timer -= 1;
        }
        if self.ch4_timer == 0 {
            let divisor = if self.ch4_divisor_code == 0 {
                8
            } else {
                (self.ch4_divisor_code as u32) * 16
            };
            self.ch4_timer = divisor << self.ch4_clock_shift;

            // LFSR tick
            let xor_result = (self.ch4_lfsr & 0x01) ^ ((self.ch4_lfsr >> 1) & 0x01);
            self.ch4_lfsr = (self.ch4_lfsr >> 1) | (xor_result << 14);

            if self.ch4_width_mode {
                self.ch4_lfsr &= !(1 << 6);
                self.ch4_lfsr |= xor_result << 6;
            }
        }
    }

    fn tick_frame_sequencer(&mut self) {
        self.frame_step = (self.frame_step + 1) % 8;

        // Length counter (steps 0, 2, 4, 6)
        if self.frame_step % 2 == 0 {
            self.tick_length_counters();
        }

        // Envelope (step 7)
        if self.frame_step == 7 {
            self.tick_envelopes();
        }

        // Sweep (steps 2, 6)
        if self.frame_step == 2 || self.frame_step == 6 {
            self.tick_sweep();
        }
    }

    fn tick_length_counters(&mut self) {
        if self.ch1_length_enabled && self.ch1_length_counter > 0 {
            self.ch1_length_counter -= 1;
            if self.ch1_length_counter == 0 {
                self.ch1_enabled = false;
            }
        }

        if self.ch2_length_enabled && self.ch2_length_counter > 0 {
            self.ch2_length_counter -= 1;
            if self.ch2_length_counter == 0 {
                self.ch2_enabled = false;
            }
        }

        if self.ch3_length_enabled && self.ch3_length_counter > 0 {
            self.ch3_length_counter -= 1;
            if self.ch3_length_counter == 0 {
                self.ch3_enabled = false;
            }
        }

        if self.ch4_length_enabled && self.ch4_length_counter > 0 {
            self.ch4_length_counter -= 1;
            if self.ch4_length_counter == 0 {
                self.ch4_enabled = false;
            }
        }
    }

    fn tick_envelopes(&mut self) {
        // Channel 1 envelope
        if self.ch1_envelope_period > 0 {
            if self.ch1_envelope_timer > 0 {
                self.ch1_envelope_timer -= 1;
            }
            if self.ch1_envelope_timer == 0 {
                self.ch1_envelope_timer = self.ch1_envelope_period;
                if self.ch1_envelope_add && self.ch1_volume < 15 {
                    self.ch1_volume += 1;
                } else if !self.ch1_envelope_add && self.ch1_volume > 0 {
                    self.ch1_volume -= 1;
                }
            }
        }

        // Channel 2 envelope
        if self.ch2_envelope_period > 0 {
            if self.ch2_envelope_timer > 0 {
                self.ch2_envelope_timer -= 1;
            }
            if self.ch2_envelope_timer == 0 {
                self.ch2_envelope_timer = self.ch2_envelope_period;
                if self.ch2_envelope_add && self.ch2_volume < 15 {
                    self.ch2_volume += 1;
                } else if !self.ch2_envelope_add && self.ch2_volume > 0 {
                    self.ch2_volume -= 1;
                }
            }
        }

        // Channel 4 envelope
        if self.ch4_envelope_period > 0 {
            if self.ch4_envelope_timer > 0 {
                self.ch4_envelope_timer -= 1;
            }
            if self.ch4_envelope_timer == 0 {
                self.ch4_envelope_timer = self.ch4_envelope_period;
                if self.ch4_envelope_add && self.ch4_volume < 15 {
                    self.ch4_volume += 1;
                } else if !self.ch4_envelope_add && self.ch4_volume > 0 {
                    self.ch4_volume -= 1;
                }
            }
        }
    }

    fn tick_sweep(&mut self) {
        if self.ch1_sweep_timer > 0 {
            self.ch1_sweep_timer -= 1;
        }

        if self.ch1_sweep_timer == 0 {
            self.ch1_sweep_timer = if self.ch1_sweep_period > 0 {
                self.ch1_sweep_period
            } else {
                8
            };

            if self.ch1_sweep_enabled && self.ch1_sweep_period > 0 {
                let new_freq = self.calculate_sweep_frequency();
                if new_freq <= 2047 && self.ch1_sweep_shift > 0 {
                    self.ch1_frequency = new_freq;
                    self.ch1_sweep_shadow = new_freq;

                    // Overflow check
                    let _ = self.calculate_sweep_frequency();
                }
            }
        }
    }

    fn calculate_sweep_frequency(&mut self) -> u16 {
        let delta = self.ch1_sweep_shadow >> self.ch1_sweep_shift;
        let new_freq = if self.ch1_sweep_negate {
            self.ch1_sweep_shadow.wrapping_sub(delta)
        } else {
            self.ch1_sweep_shadow.wrapping_add(delta)
        };

        if new_freq > 2047 {
            self.ch1_enabled = false;
        }

        new_freq
    }

    fn generate_sample_output(&mut self, memory: &Memory) {
        let nr50 = memory.data[io::NR50 as usize];
        let nr51 = memory.data[io::NR51 as usize];

        let left_volume = ((nr50 >> 4) & 0x07) as f32 / 7.0;
        let right_volume = (nr50 & 0x07) as f32 / 7.0;

        let mut left = 0.0f32;
        let mut right = 0.0f32;

        // Channel 1
        if self.ch1_enabled && self.ch1_dac_enabled {
            let duty = (memory.data[io::NR11 as usize] >> 6) as usize;
            let sample = DUTY_TABLE[duty][self.ch1_duty_position as usize] as f32;
            let output = sample * (self.ch1_volume as f32 / 15.0);

            if nr51 & 0x10 != 0 {
                left += output;
            }
            if nr51 & 0x01 != 0 {
                right += output;
            }
        }

        // Channel 2
        if self.ch2_enabled && self.ch2_dac_enabled {
            let duty = (memory.data[io::NR21 as usize] >> 6) as usize;
            let sample = DUTY_TABLE[duty][self.ch2_duty_position as usize] as f32;
            let output = sample * (self.ch2_volume as f32 / 15.0);

            if nr51 & 0x20 != 0 {
                left += output;
            }
            if nr51 & 0x02 != 0 {
                right += output;
            }
        }

        // Channel 3
        if self.ch3_enabled && self.ch3_dac_enabled {
            let shift = match self.ch3_volume_code {
                0 => 4, // Mute
                1 => 0, // 100%
                2 => 1, // 50%
                3 => 2, // 25%
                _ => 4,
            };
            let output = ((self.ch3_sample_buffer >> shift) as f32) / 15.0;

            if nr51 & 0x40 != 0 {
                left += output;
            }
            if nr51 & 0x04 != 0 {
                right += output;
            }
        }

        // Channel 4
        if self.ch4_enabled && self.ch4_dac_enabled {
            let sample = if self.ch4_lfsr & 0x01 == 0 { 1.0 } else { 0.0 };
            let output = sample * (self.ch4_volume as f32 / 15.0);

            if nr51 & 0x80 != 0 {
                left += output;
            }
            if nr51 & 0x08 != 0 {
                right += output;
            }
        }

        // Mix and apply master volume
        left = (left / 4.0) * left_volume;
        right = (right / 4.0) * right_volume;

        // Apply high-pass filter to remove DC offset and reduce pops
        // This simulates the capacitor in the Game Boy's audio output
        const HPF_FACTOR: f32 = 0.999;
        self.hpf_left = self.hpf_left * HPF_FACTOR + left;
        self.hpf_right = self.hpf_right * HPF_FACTOR + right;
        let left_out = left - self.hpf_left * (1.0 - HPF_FACTOR);
        let right_out = right - self.hpf_right * (1.0 - HPF_FACTOR);

        // Output stereo sample (interleaved) with slight volume reduction
        self.buffer.push(left_out * 0.5);
        self.buffer.push(right_out * 0.5);
    }

    /// Trigger channel 1
    pub fn trigger_ch1(&mut self, memory: &Memory) {
        let nr11 = memory.data[io::NR11 as usize];
        let nr12 = memory.data[io::NR12 as usize];
        let nr13 = memory.data[io::NR13 as usize];
        let nr14 = memory.data[io::NR14 as usize];

        self.ch1_enabled = self.ch1_dac_enabled;
        self.ch1_length_counter = 64 - (nr11 & 0x3F);
        self.ch1_frequency = (nr13 as u16) | (((nr14 & 0x07) as u16) << 8);
        self.ch1_timer = (2048 - self.ch1_frequency) * 4;
        self.ch1_volume = nr12 >> 4;
        self.ch1_volume_initial = self.ch1_volume;
        self.ch1_envelope_period = nr12 & 0x07;
        self.ch1_envelope_timer = self.ch1_envelope_period;
        self.ch1_envelope_add = nr12 & 0x08 != 0;

        // Sweep
        let nr10 = memory.data[io::NR10 as usize];
        self.ch1_sweep_shadow = self.ch1_frequency;
        self.ch1_sweep_timer = if self.ch1_sweep_period > 0 {
            self.ch1_sweep_period
        } else {
            8
        };
        self.ch1_sweep_enabled = self.ch1_sweep_period > 0 || self.ch1_sweep_shift > 0;

        if self.ch1_sweep_shift > 0 {
            let _ = self.calculate_sweep_frequency();
        }
    }

    /// Trigger channel 2
    pub fn trigger_ch2(&mut self, memory: &Memory) {
        let nr21 = memory.data[io::NR21 as usize];
        let nr22 = memory.data[io::NR22 as usize];
        let nr23 = memory.data[io::NR23 as usize];
        let nr24 = memory.data[io::NR24 as usize];

        self.ch2_enabled = self.ch2_dac_enabled;
        self.ch2_length_counter = 64 - (nr21 & 0x3F);
        self.ch2_frequency = (nr23 as u16) | (((nr24 & 0x07) as u16) << 8);
        self.ch2_timer = (2048 - self.ch2_frequency) * 4;
        self.ch2_volume = nr22 >> 4;
        self.ch2_volume_initial = self.ch2_volume;
        self.ch2_envelope_period = nr22 & 0x07;
        self.ch2_envelope_timer = self.ch2_envelope_period;
        self.ch2_envelope_add = nr22 & 0x08 != 0;
    }

    /// Trigger channel 3
    pub fn trigger_ch3(&mut self, memory: &Memory) {
        let nr31 = memory.data[io::NR31 as usize];
        let nr33 = memory.data[io::NR33 as usize];
        let nr34 = memory.data[io::NR34 as usize];

        self.ch3_enabled = self.ch3_dac_enabled;
        self.ch3_length_counter = 256 - (nr31 as u16);
        self.ch3_frequency = (nr33 as u16) | (((nr34 & 0x07) as u16) << 8);
        self.ch3_timer = (2048 - self.ch3_frequency) * 2;
        self.ch3_position = 0;
    }

    /// Trigger channel 4
    pub fn trigger_ch4(&mut self, memory: &Memory) {
        let nr41 = memory.data[io::NR41 as usize];
        let nr42 = memory.data[io::NR42 as usize];

        self.ch4_enabled = self.ch4_dac_enabled;
        self.ch4_length_counter = 64 - (nr41 & 0x3F);
        self.ch4_lfsr = 0x7FFF;
        self.ch4_volume = nr42 >> 4;
        self.ch4_volume_initial = self.ch4_volume;
        self.ch4_envelope_period = nr42 & 0x07;
        self.ch4_envelope_timer = self.ch4_envelope_period;
        self.ch4_envelope_add = nr42 & 0x08 != 0;

        let divisor = if self.ch4_divisor_code == 0 {
            8
        } else {
            (self.ch4_divisor_code as u32) * 16
        };
        self.ch4_timer = divisor << self.ch4_clock_shift;
    }

    /// Clear audio buffer
    pub fn clear_buffer(&mut self) {
        self.buffer.clear();
    }

    /// Take all samples from buffer (drains it)
    pub fn take_samples(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.buffer)
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

