/// Automated test runner for Game Boy test ROMs
///
/// Supports multiple test ROM formats:
/// 1. Blargg tests - output via serial port, "Passed"/"Failed" in output
/// 2. Mooneye tests - execute LD B,B when done, Fibonacci registers on success

use crate::cpu::Cpu;
use crate::memory::Memory;
use crate::ppu::Ppu;
use crate::timer::Timer;

/// Maximum cycles to run a test before timing out
const MAX_CYCLES: u64 = 500_000_000; // ~120 seconds of emulated time

/// Mooneye Fibonacci success signature
const MOONEYE_B: u8 = 3;
const MOONEYE_C: u8 = 5;
const MOONEYE_D: u8 = 8;
const MOONEYE_E: u8 = 13;
const MOONEYE_H: u8 = 21;
const MOONEYE_L: u8 = 34;

/// Result of running a test
#[derive(Debug)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub output: String,
    pub cycles: u64,
    pub error: Option<String>,
}

/// Run a single test ROM and return the result
pub fn run_test(rom_path: &str) -> TestResult {
    let name = std::path::Path::new(rom_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| rom_path.to_string());

    // Load ROM
    let rom = match std::fs::read(rom_path) {
        Ok(data) => data,
        Err(e) => {
            return TestResult {
                name,
                passed: false,
                output: String::new(),
                cycles: 0,
                error: Some(format!("Failed to load ROM: {}", e)),
            };
        }
    };

    // Initialize emulator components
    let mut cpu = Cpu::new();
    let mut memory = Memory::new();
    let mut ppu = Ppu::new();
    let mut timer = Timer::new();

    memory.load_rom(&rom);
    cpu.reset();

    // Serial output buffer
    let mut serial_output = String::new();
    let mut total_cycles: u64 = 0;
    
    // Track previous PC for Mooneye LD B,B detection
    let mut prev_pc: u16 = 0;

    // Run the test
    loop {
        // Check for timeout
        if total_cycles >= MAX_CYCLES {
            return TestResult {
                name,
                passed: false,
                output: serial_output,
                cycles: total_cycles,
                error: Some("Test timed out".to_string()),
            };
        }

        // Handle interrupts
        handle_interrupts(&mut cpu, &mut memory);

        // Save PC before execution
        prev_pc = cpu.pc;

        // Execute one instruction
        let cycles = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cpu.step(&mut memory)
        })) {
            Ok(c) => c,
            Err(e) => {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                return TestResult {
                    name,
                    passed: false,
                    output: serial_output,
                    cycles: total_cycles,
                    error: Some(format!("CPU panic: {}", msg)),
                };
            }
        };

        total_cycles += cycles as u64;

        // Check for Mooneye test completion (LD B, B = 0x40 in an infinite loop)
        // Mooneye tests end with: LD B, B followed by JR -2 (infinite loop)
        // So we check if the current instruction is LD B,B and the next is JR -2
        let executed_opcode = memory.read_byte(prev_pc);
        if executed_opcode == 0x40 {
            // Check if followed by JR -2 (0x18 0xFE) which is the Mooneye termination pattern
            let next_opcode = memory.read_byte(prev_pc.wrapping_add(1));
            let jr_offset = memory.read_byte(prev_pc.wrapping_add(2));
            
            if next_opcode == 0x18 && jr_offset == 0xFE {
                // This is the Mooneye termination pattern
                let is_fibonacci = cpu.b == MOONEYE_B
                    && cpu.c == MOONEYE_C
                    && cpu.d == MOONEYE_D
                    && cpu.e == MOONEYE_E
                    && cpu.h == MOONEYE_H
                    && cpu.l == MOONEYE_L;
                
                return TestResult {
                    name,
                    passed: is_fibonacci,
                    output: serial_output,
                    cycles: total_cycles,
                    error: if !is_fibonacci {
                        Some(format!(
                            "Mooneye: B={} C={} D={} E={} H={} L={}",
                            cpu.b, cpu.c, cpu.d, cpu.e, cpu.h, cpu.l
                        ))
                    } else {
                        None
                    },
                };
            }
        }

        // Update timer
        timer.tick(&mut memory, cycles);

        // Update PPU (simplified - we don't need full rendering for tests)
        ppu.tick(&mut memory, cycles);
        
        // Update DMA (once per cycle)
        for _ in 0..cycles {
            memory.tick_dma();
        }

        // Check serial output
        // When SC (0xFF02) has bit 7 set and bit 0 set, a byte is being sent
        let sc = memory.data[0xFF02];
        if sc == 0x81 {
            let sb = memory.data[0xFF01];
            serial_output.push(sb as char);
            memory.data[0xFF02] = 0; // Clear transfer flag

            // Check for test completion
            if serial_output.contains("Passed") {
                return TestResult {
                    name,
                    passed: true,
                    output: serial_output,
                    cycles: total_cycles,
                    error: None,
                };
            }
            if serial_output.contains("Failed") {
                return TestResult {
                    name,
                    passed: false,
                    output: serial_output,
                    cycles: total_cycles,
                    error: None,
                };
            }
        }

        // Also check memory signature for test completion
        // Blargg tests write 0 to 0xA000 on success, non-zero on failure
        // And they set specific patterns when done
        if memory.data[0xA001] == 0xDE
            && memory.data[0xA002] == 0xB0
            && memory.data[0xA003] == 0x61
        {
            let status = memory.data[0xA000];
            return TestResult {
                name,
                passed: status == 0,
                output: serial_output,
                cycles: total_cycles,
                error: if status != 0 {
                    Some(format!("Test failed with status: {}", status))
                } else {
                    None
                },
            };
        }
    }
}

/// Handle pending interrupts
fn handle_interrupts(cpu: &mut Cpu, memory: &mut Memory) {
    // Wake from HALT if any interrupt is pending
    if memory.pending_interrupts() != 0 {
        cpu.halted = false;
    }

    if !cpu.ime {
        return;
    }

    let pending = memory.pending_interrupts();
    if pending == 0 {
        return;
    }

    cpu.ime = false;

    // Push PC onto stack
    let pc = cpu.pc;
    cpu.sp = cpu.sp.wrapping_sub(1);
    memory.data[cpu.sp as usize] = (pc >> 8) as u8;
    cpu.sp = cpu.sp.wrapping_sub(1);
    memory.data[cpu.sp as usize] = pc as u8;

    // Jump to interrupt handler (priority order)
    use crate::memory::interrupts;
    if pending & interrupts::VBLANK != 0 {
        memory.clear_interrupt(interrupts::VBLANK);
        cpu.pc = 0x0040;
    } else if pending & interrupts::LCD_STAT != 0 {
        memory.clear_interrupt(interrupts::LCD_STAT);
        cpu.pc = 0x0048;
    } else if pending & interrupts::TIMER != 0 {
        memory.clear_interrupt(interrupts::TIMER);
        cpu.pc = 0x0050;
    } else if pending & interrupts::SERIAL != 0 {
        memory.clear_interrupt(interrupts::SERIAL);
        cpu.pc = 0x0058;
    } else if pending & interrupts::JOYPAD != 0 {
        memory.clear_interrupt(interrupts::JOYPAD);
        cpu.pc = 0x0060;
    }
}

/// Run all tests in a directory or a single test file
pub fn run_all_tests(test_path: &str) -> Vec<TestResult> {
    let mut results = Vec::new();

    let path = std::path::Path::new(test_path);
    
    // If it's a single .gb file, run just that test
    if path.is_file() && path.extension().map(|e| e == "gb").unwrap_or(false) {
        let result = run_test(test_path);
        results.push(result);
        return results;
    }

    let paths: Vec<_> = std::fs::read_dir(test_path)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "gb")
                .unwrap_or(false)
        })
        .collect();

    for entry in paths {
        let path = entry.path();
        println!("Running test: {}", path.display());
        let result = run_test(path.to_str().unwrap());
        println!(
            "  {} - {} cycles",
            if result.passed { "PASSED ✓" } else { "FAILED ✗" },
            result.cycles
        );
        if let Some(ref err) = result.error {
            println!("  Error: {}", err);
        }
        if !result.output.is_empty() {
            println!("  Output: {}", result.output.trim());
        }
        results.push(result);
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn test_blargg_cpu_instrs_01() {
        let result = run_test("test_roms/blargg/cpu_instrs/individual/01-special.gb");
        println!("Output: {}", result.output);
        if let Some(ref err) = result.error {
            println!("Error: {}", err);
        }
        assert!(result.passed, "Test 01-special failed");
    }

    #[test]
    #[ignore]
    fn test_blargg_cpu_instrs_03() {
        let result = run_test("test_roms/blargg/cpu_instrs/individual/03-op sp,hl.gb");
        println!("Output: {}", result.output);
        assert!(result.passed, "Test 03-op sp,hl failed");
    }

    #[test]
    #[ignore]
    fn test_blargg_cpu_instrs_04() {
        let result = run_test("test_roms/blargg/cpu_instrs/individual/04-op r,imm.gb");
        println!("Output: {}", result.output);
        assert!(result.passed, "Test 04-op r,imm failed");
    }

    #[test]
    #[ignore]
    fn test_blargg_cpu_instrs_06() {
        let result = run_test("test_roms/blargg/cpu_instrs/individual/06-ld r,r.gb");
        println!("Output: {}", result.output);
        assert!(result.passed, "Test 06-ld r,r failed");
    }
}

