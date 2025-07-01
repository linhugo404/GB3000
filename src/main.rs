mod cpu;
mod memory;

use cpu::Cpu;
use memory::Memory;

/// Entry point for the emulator. Currently this only creates the CPU and memory
/// structures and performs a single step as a placeholder.
fn main() {
    let mut cpu = Cpu::new();
    let mut memory = Memory::new();

    cpu.reset();
    // In the future we will load a ROM into memory here and run the emulation
    // loop. For now we just perform a single no-op step.
    cpu.step(&mut memory.data);

    println!("CPU after reset: {:?}", cpu);
}
