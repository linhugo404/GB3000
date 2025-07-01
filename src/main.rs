mod cpu;
mod memory;

use cpu::Cpu;
use memory::Memory;
use std::env;
use std::fs;

/// Entry point for the emulator. This creates the CPU and memory structures and
/// runs a very naive execution loop.
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

    // Run the emulation loop until the process is terminated or an unimplemented
    // instruction causes a panic.
    loop {
        cpu.step(&mut memory.data);
    }
}
