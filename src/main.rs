mod apu;
mod cpu;
mod memory;
mod ppu;
mod test_runner;
mod timer;

use apu::Apu;
use cpu::Cpu;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use memory::{interrupts, Memory};
use minifb::{Key, Scale, Window, WindowOptions};
use ppu::{Ppu, SCREEN_HEIGHT, SCREEN_WIDTH};
use std::env;
use std::fs;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use timer::Timer;

/// Game Boy color palette (grayscale for clearer visibility)
const PALETTE: [u32; 4] = [
    0xFFFFFFFF, // White (color 0)
    0xFFAAAAAA, // Light gray (color 1)
    0xFF555555, // Dark gray (color 2)
    0xFF000000, // Black (color 3)
];

/// Alternative palette (classic green-ish DMG colors)
#[allow(dead_code)]
const PALETTE_GREEN: [u32; 4] = [
    0xFF9BBC0F, // Lightest
    0xFF8BAC0F, // Light
    0xFF306230, // Dark
    0xFF0F380F, // Darkest
];

/// CPU clock speed (4.194304 MHz)
const CPU_CLOCK_HZ: u32 = 4_194_304;

/// Cycles per frame (70224 cycles per frame at ~59.7 FPS)
const CYCLES_PER_FRAME: u32 = 70224;

/// Target frame time in nanoseconds (~16.74ms)
const FRAME_TIME_NS: u64 = 1_000_000_000 / 60;

/// The main emulator struct that ties all components together
pub struct Emulator {
    cpu: Cpu,
    memory: Memory,
    ppu: Ppu,
    apu: Apu,
    timer: Timer,
}

impl Emulator {
    pub fn new() -> Self {
        Self {
            cpu: Cpu::new(),
            memory: Memory::new(),
            ppu: Ppu::new(),
            apu: Apu::new(),
            timer: Timer::new(),
        }
    }

    pub fn load_rom(&mut self, rom: &[u8]) {
        self.memory.load_rom(rom);
    }

    pub fn reset(&mut self) {
        self.cpu.reset();
        self.ppu.reset();
        self.apu.reset();
        self.timer.reset();
    }

    /// Run emulation for one frame
    pub fn run_frame(&mut self) {
        let mut cycles_this_frame = 0u32;

        while cycles_this_frame < CYCLES_PER_FRAME {
            let cycles = self.step();
            cycles_this_frame += cycles;

            // Check if frame is ready
            if self.ppu.frame_ready {
                self.ppu.frame_ready = false;
                break;
            }
        }
    }

    /// Execute a single CPU step and update all subsystems
    pub fn step(&mut self) -> u32 {
        // Handle interrupts (may consume 20 cycles if interrupt is serviced)
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

        // Update timer
        self.timer.tick(&mut self.memory, cycles);

        // Update PPU
        self.ppu.tick(&mut self.memory, cycles);

        // Update APU
        self.apu.tick(&mut self.memory, cycles);

        // Handle DMA
        for _ in 0..cycles {
            self.memory.tick_dma();
        }

        cycles + intr_cycles
    }

    /// Handle pending interrupts. Returns cycles consumed (20 if interrupt was serviced)
    fn handle_interrupts(&mut self) -> u32 {
        // Wake from HALT if any interrupt is pending (even if IME is disabled)
        if self.memory.pending_interrupts() != 0 {
            self.cpu.halted = false;
        }

        // Only service interrupts if IME is enabled
        if !self.cpu.ime {
            return 0;
        }

        let pending = self.memory.pending_interrupts();
        if pending == 0 {
            return 0;
        }

        // Disable interrupts
        self.cpu.ime = false;

        // Push PC onto stack
        let pc = self.cpu.pc;
        self.cpu.sp = self.cpu.sp.wrapping_sub(1);
        self.memory.data[self.cpu.sp as usize] = (pc >> 8) as u8;
        self.cpu.sp = self.cpu.sp.wrapping_sub(1);
        self.memory.data[self.cpu.sp as usize] = pc as u8;

        // Jump to interrupt handler (priority order)
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
        
        // Interrupt dispatch takes 5 M-cycles = 20 T-cycles
        20
    }

    /// Update joypad state from window keys
    pub fn update_joypad(&mut self, window: &Window) {
        // Joypad state: bits are active LOW (0 = pressed)
        // Bits 7-4: Start, Select, B, A
        // Bits 3-0: Down, Up, Left, Right
        let mut state = 0xFFu8;

        // Direction keys
        if window.is_key_down(Key::Right) {
            state &= !0x01;
        }
        if window.is_key_down(Key::Left) {
            state &= !0x02;
        }
        if window.is_key_down(Key::Up) {
            state &= !0x04;
        }
        if window.is_key_down(Key::Down) {
            state &= !0x08;
        }

        // Button keys
        if window.is_key_down(Key::Z) {
            // A button
            state &= !0x10;
        }
        if window.is_key_down(Key::X) {
            // B button
            state &= !0x20;
        }
        if window.is_key_down(Key::Space) {
            // Select
            state &= !0x40;
        }
        if window.is_key_down(Key::Enter) {
            // Start
            state &= !0x80;
        }

        self.memory.set_joypad(state);
    }

    /// Get the framebuffer as ARGB pixels
    pub fn get_framebuffer(&self) -> Vec<u32> {
        self.ppu
            .framebuffer
            .iter()
            .map(|&color| PALETTE[color as usize & 0x03])
            .collect()
    }

    /// Take audio samples from APU buffer
    pub fn take_audio_samples(&mut self) -> Vec<f32> {
        self.apu.take_samples()
    }
}

impl Default for Emulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Audio buffer size constants
const AUDIO_BUFFER_SIZE: usize = 4096;
const AUDIO_LOW_WATER: usize = 1024;

/// Set up audio output stream using cpal
fn setup_audio(audio_buffer: Arc<Mutex<VecDeque<f32>>>) -> Option<cpal::Stream> {
    let host = cpal::default_host();
    let device = match host.default_output_device() {
        Some(d) => d,
        None => {
            eprintln!("Warning: No audio output device found");
            return None;
        }
    };

    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(apu::SAMPLE_RATE),
        buffer_size: cpal::BufferSize::Default,
    };

    // Track last sample for smooth transitions during underrun
    let mut last_sample = 0.0f32;

    let stream = device
        .build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut buffer = audio_buffer.lock().unwrap();
                
                for sample in data.iter_mut() {
                    if let Some(s) = buffer.pop_front() {
                        *sample = s;
                        last_sample = s;
                    } else {
                        // Fade to silence on underrun to avoid pops
                        last_sample *= 0.9;
                        *sample = last_sample;
                    }
                }
            },
            |err| eprintln!("Audio stream error: {}", err),
            None,
        )
        .ok()?;

    stream.play().ok()?;
    Some(stream)
}

fn print_rom_info(rom: &[u8]) {
    if rom.len() < 0x150 {
        println!("ROM too small to read header");
        return;
    }

    // Title (0x0134-0x0143)
    let title: String = rom[0x0134..0x0144]
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect();

    // Cartridge type (0x0147)
    let cart_type = rom[0x0147];
    let cart_type_str = match cart_type {
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
    };

    // ROM size (0x0148)
    let rom_size = match rom[0x0148] {
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
    };

    // RAM size (0x0149)
    let ram_size = match rom[0x0149] {
        0x00 => "None",
        0x01 => "2 KB",
        0x02 => "8 KB",
        0x03 => "32 KB",
        0x04 => "128 KB",
        0x05 => "64 KB",
        _ => "Unknown",
    };

    println!("╔══════════════════════════════════════╗");
    println!("║           GB3000 Emulator            ║");
    println!("╠══════════════════════════════════════╣");
    println!("║ Title: {:30} ║", title);
    println!("║ Type:  {:30} ║", cart_type_str);
    println!("║ ROM:   {:30} ║", rom_size);
    println!("║ RAM:   {:30} ║", ram_size);
    println!("╚══════════════════════════════════════╝");
    println!();
    println!("Controls:");
    println!("  Arrow Keys = D-Pad");
    println!("  Z = A Button");
    println!("  X = B Button");
    println!("  Enter = Start");
    println!("  Space = Select");
    println!("  Escape = Quit");
    println!();
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <rom.gb>", args[0]);
        eprintln!("       {} --test [test_dir]", args[0]);
        eprintln!();
        eprintln!("GB3000 - A Game Boy Emulator");
        eprintln!();
        eprintln!("Options:");
        eprintln!("  <rom.gb>     Run a Game Boy ROM file");
        eprintln!("  --test       Run Blargg test ROMs (default: test_roms/blargg/cpu_instrs/individual/)");
        std::process::exit(1);
    }

    // Check for test mode
    if args[1] == "--test" {
        let test_dir = args.get(2).map(|s| s.as_str()).unwrap_or("test_roms/blargg/cpu_instrs/individual");
        println!("╔══════════════════════════════════════╗");
        println!("║      GB3000 Test Runner              ║");
        println!("╚══════════════════════════════════════╝");
        println!();
        println!("Running tests from: {}", test_dir);
        println!();
        
        let results = test_runner::run_all_tests(test_dir);
        
        println!();
        println!("════════════════════════════════════════");
        println!("                SUMMARY                 ");
        println!("════════════════════════════════════════");
        
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.iter().filter(|r| !r.passed).count();
        
        for result in &results {
            let status = if result.passed { "✓ PASS" } else { "✗ FAIL" };
            println!("{} {} ({} cycles)", status, result.name, result.cycles);
            if !result.passed {
                if let Some(ref err) = result.error {
                    println!("  Error: {}", err);
                }
                if !result.output.is_empty() {
                    println!("  Output: {}", result.output.chars().take(100).collect::<String>());
                }
            }
        }
        
        println!();
        println!("Passed: {}/{}", passed, results.len());
        println!("Failed: {}/{}", failed, results.len());
        
        if failed > 0 {
            std::process::exit(1);
        }
        return;
    }

    let rom_path = &args[1];
    let rom = fs::read(rom_path).expect("Failed to read ROM file");

    print_rom_info(&rom);

    // Create emulator and load ROM
    let mut emulator = Emulator::new();
    emulator.load_rom(&rom);
    emulator.reset();

    // Create window
    let mut window = Window::new(
        "GB3000 - Game Boy Emulator",
        SCREEN_WIDTH,
        SCREEN_HEIGHT,
        WindowOptions {
            scale: Scale::X4,
            ..WindowOptions::default()
        },
    )
    .expect("Failed to create window");

    // Set target FPS (limit to ~60 FPS)
    window.set_target_fps(60);

    // Set up audio output with ring buffer
    let audio_buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::with_capacity(AUDIO_BUFFER_SIZE)));
    let audio_buffer_clone = Arc::clone(&audio_buffer);

    let _audio_stream = setup_audio(audio_buffer_clone);

    let mut frame_count = 0u64;
    let start_time = Instant::now();
    let mut last_fps_time = Instant::now();

    // Main emulation loop
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let frame_start = Instant::now();

        // Update joypad
        emulator.update_joypad(&window);

        // Run one frame of emulation
        emulator.run_frame();

        // Get framebuffer and display
        let framebuffer = emulator.get_framebuffer();
        window
            .update_with_buffer(&framebuffer, SCREEN_WIDTH, SCREEN_HEIGHT)
            .expect("Failed to update window");

        // Send audio samples to audio thread
        let samples = emulator.take_audio_samples();
        if !samples.is_empty() {
            if let Ok(mut buffer) = audio_buffer.lock() {
                // Add samples to ring buffer
                for sample in samples {
                    buffer.push_back(sample);
                }
                // Limit buffer size to prevent latency buildup
                while buffer.len() > AUDIO_BUFFER_SIZE {
                    buffer.pop_front();
                }
            }
        }

        frame_count += 1;

        // Print FPS every second
        if last_fps_time.elapsed() >= Duration::from_secs(1) {
            let elapsed = start_time.elapsed().as_secs_f64();
            let fps = frame_count as f64 / elapsed;
            println!("FPS: {:.1}", fps);
            last_fps_time = Instant::now();
        }

        // Frame timing (aim for ~60 FPS)
        let frame_time = frame_start.elapsed();
        let target_time = Duration::from_nanos(FRAME_TIME_NS);
        if frame_time < target_time {
            spin_sleep::sleep(target_time - frame_time);
        }
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    println!(
        "Emulation ended. {} frames in {:.2}s ({:.1} FPS average)",
        frame_count,
        elapsed,
        frame_count as f64 / elapsed
    );
}
