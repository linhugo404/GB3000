mod cpu;
mod memory;

use cpu::Cpu;
use memory::Memory;
use std::env;
use std::fs;

/// Entry point for the emulator. Currently this only creates the CPU and memory
/// structures and performs a single step as a placeholder.
fn main() {
    let args: Vec<String> = env::args().collect();
    let rom_path = args.get(1);

    let mut cpu = Cpu::new();
    let mut memory = Memory::new();

    if let Some(path) = rom_path {
        let rom = fs::read(path).expect("failed to read ROM");
        memory.load_rom(&rom);
    }

    cpu.reset();

    // Run a few instructions as a placeholder until a real emulation loop is
    // implemented.
    for _ in 0..100 {
        cpu.step(&mut memory.data);
    }

    println!("CPU after execution: {:?}", cpu);
}
